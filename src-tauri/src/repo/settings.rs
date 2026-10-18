//! 设置仓储（契约 3.1 settings 表：键值对，本机身份与偏好，不参与数据包合并）。

use rusqlite::{params, Connection};

use crate::commands::app::Setting;
use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};

use super::map_db_err;

/// 全部设置（按 key 升序，契约 4.1 get_settings）。
pub fn get_all(conn: &Connection) -> Result<Vec<Setting>, CommandError> {
    let mut stmt = conn
        .prepare("SELECT key, value, updated_at FROM settings ORDER BY key")
        .map_err(|e| map_db_err(&e, "读取设置"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Setting {
                key: r.get(0)?,
                value: r.get(1)?,
                updated_at: r.get(2)?,
            })
        })
        .map_err(|e| map_db_err(&e, "读取设置"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "读取设置"))?;
    Ok(rows)
}

/// 读单个键；不存在返回 None（不报错，供 device_id 等首启初始化路径使用）。
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, CommandError> {
    match conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get::<_, String>(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(map_db_err(&e, "读取设置")),
    }
}

/// 写入设置（UPSERT，同步刷新 updated_at）。
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), CommandError> {
    if key.trim().is_empty() {
        return Err(CommandError::new(ErrorCode::Validation, "设置键不能为空"));
    }
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now_rfc3339()],
    )
    .map_err(|e| map_db_err(&e, "写入设置"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn set_get_and_upsert() {
        let db = test_db();
        let conn = db.lock();

        assert_eq!(get(&conn, "missing").unwrap(), None);

        set(&conn, "backup_remind_days", "7").unwrap();
        assert_eq!(get(&conn, "backup_remind_days").unwrap().unwrap(), "7");

        // UPSERT：再次写入同键覆盖值
        set(&conn, "backup_remind_days", "14").unwrap();
        assert_eq!(get(&conn, "backup_remind_days").unwrap().unwrap(), "14");

        let all = get_all(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].key, "backup_remind_days");
        assert_eq!(all[0].value, "14");
        chrono::DateTime::parse_from_rfc3339(&all[0].updated_at).unwrap();
    }

    #[test]
    fn empty_key_is_validation_error() {
        let db = test_db();
        let conn = db.lock();
        let err = set(&conn, "  ", "x").unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
    }
}
