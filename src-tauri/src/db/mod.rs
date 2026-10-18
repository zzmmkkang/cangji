//! 数据库连接管理（契约 1.1 / 3.5）：
//! - 打开 `app_local_data_dir()/cangji.db`（单测用 `open_in_memory`）
//! - PRAGMA：journal_mode=WAL、busy_timeout=5000、foreign_keys=ON、synchronous=NORMAL
//! - `Arc<Mutex<Connection>>` 单连接经 `tauri::State` 托管（不引入连接池，契约 1.1）

#![allow(dead_code)]

pub mod migrations;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::{CommandError, ErrorCode};

/// 单连接句柄：`0` = 连接本体，`1` = 库文件路径（内存库为 `":memory:"`，供 get_app_info 下发）。
pub struct Db(pub Arc<Mutex<Connection>>, pub String);

impl Db {
    /// 打开库文件：建父目录 → 设 PRAGMA → 执行迁移。失败返回 Err，由启动侧中止（契约 3.6）。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CommandError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    CommandError::new(ErrorCode::Io, format!("创建数据库目录失败：{e}"))
                })?;
            }
        }
        let conn = Connection::open(path).map_err(|e| {
            CommandError::new(ErrorCode::Db, format!("打开数据库失败：{e}"))
        })?;
        log::info!("数据库文件：{}", path.display());
        Self::init(conn, path.to_string_lossy().into_owned())
    }

    /// 内存库（单测用）：同样执行 PRAGMA 与迁移（视图/PRAGMA 在内存库均生效）。
    pub fn open_in_memory() -> Result<Self, CommandError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            CommandError::new(ErrorCode::Db, format!("打开内存数据库失败：{e}"))
        })?;
        Self::init(conn, ":memory:".to_string())
    }

    fn init(mut conn: Connection, path: String) -> Result<Self, CommandError> {
        conn.execute_batch(concat!(
            "PRAGMA journal_mode=WAL;",
            "PRAGMA foreign_keys=ON;",
            "PRAGMA busy_timeout=5000;",
            "PRAGMA synchronous=NORMAL;"
        ))
        .map_err(|e| CommandError::new(ErrorCode::Db, format!("设置 PRAGMA 失败：{e}")))?;
        migrations::run(&mut conn)?;
        Ok(Self(Arc::new(Mutex::new(conn)), path))
    }

    /// 获取连接锁（互斥锁中毒时取回内部数据继续；单连接模型下中毒仅可能来自 panic 中断）。
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 库文件路径（内存库为 ":memory:"）。
    pub fn path(&self) -> &str {
        &self.1
    }
}

/// 统一时间戳：UTC RFC3339 毫秒，如 `2026-10-17T06:30:00.123Z`（契约 7.3）。
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 测试用内存库。
#[cfg(test)]
pub(crate) fn test_db() -> Db {
    Db::open_in_memory().expect("测试内存库初始化失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_applies_pragmas_and_migration() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock();
        // 契约 3.5：foreign_keys=ON、busy_timeout=5000、synchronous=NORMAL
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let busy: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, 5000);
        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 1); // NORMAL
        assert_eq!(db.path(), ":memory:");
        // 迁移已在打开时执行
        assert_eq!(migrations::current_version(&conn).unwrap(), 1);
    }

    #[test]
    fn now_rfc3339_is_utc_millis_with_z() {
        let now = now_rfc3339();
        // 形如 2026-10-17T06:30:00.123Z（24 字符，毫秒精度，UTC 以 Z 结尾）
        assert_eq!(now.len(), 24, "RFC3339 毫秒格式：{now}");
        assert!(now.ends_with('Z'), "{now}");
        assert!(now.contains('.'), "{now}");
        chrono::DateTime::parse_from_rfc3339(&now).unwrap();
    }
}
