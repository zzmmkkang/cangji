//! 设备身份（契约 7.2）：
//! - 首次启动 settings 无 `device_id` → 生成 UUID v4 写入，此后永不变
//! - `device_name` 默认取系统名（Windows 读 COMPUTERNAME；Android 用 "Android"），
//!   用户可在数据共账页修改（set_setting），二者随 get_app_info 下发、写入数据包头

use rusqlite::Connection;
use uuid::Uuid;

use crate::error::CommandError;
use crate::repo::settings;

const KEY_DEVICE_ID: &str = "device_id";
const KEY_DEVICE_NAME: &str = "device_name";

/// 读取本机 device_id；settings 缺失时生成 UUID v4 并持久化（此后永不变）。
pub fn device_id(conn: &Connection) -> Result<String, CommandError> {
    if let Some(existing) = settings::get(conn, KEY_DEVICE_ID)? {
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    settings::set(conn, KEY_DEVICE_ID, &id)?;
    Ok(id)
}

/// 读取本机 device_name；缺失时按系统名初始化并持久化（用户改名后以 settings 为准）。
pub fn device_name(conn: &Connection) -> Result<String, CommandError> {
    if let Some(existing) = settings::get(conn, KEY_DEVICE_NAME)? {
        return Ok(existing);
    }
    let name = default_device_name();
    settings::set(conn, KEY_DEVICE_NAME, &name)?;
    Ok(name)
}

/// 默认设备名：Windows 读 COMPUTERNAME、Android 回退 "Android"，再退 HOSTNAME / 常量。
fn default_device_name() -> String {
    if cfg!(target_os = "android") {
        return "Android".to_string();
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "未知设备".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn device_id_is_uuid_v4_and_persistent() {
        let db = test_db();
        {
            let conn = db.lock();
            let id = device_id(&conn).unwrap();
            let parsed = Uuid::parse_str(&id).unwrap();
            assert_eq!(parsed.get_version_num(), 4, "device_id 必须是 UUID v4");
            // 再读稳定，不重新生成
            assert_eq!(device_id(&conn).unwrap(), id);
        }
        // 重新锁定（同一库）仍稳定
        let conn = db.lock();
        let again = device_id(&conn).unwrap();
        let first = settings::get(&conn, KEY_DEVICE_ID).unwrap().unwrap();
        assert_eq!(again, first);
    }

    #[test]
    fn device_name_defaults_and_follows_setting() {
        let db = test_db();
        let conn = db.lock();
        let name = device_name(&conn).unwrap();
        assert!(!name.trim().is_empty());
        assert_eq!(device_name(&conn).unwrap(), name, "首次初始化后跟随 settings");

        // 用户在数据共账页改名 → device_name 跟随
        settings::set(&conn, KEY_DEVICE_NAME, "我的仓库").unwrap();
        assert_eq!(device_name(&conn).unwrap(), "我的仓库");
    }
}
