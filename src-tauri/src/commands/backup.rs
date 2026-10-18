//! 备份域命令（契约 4.7）。共 1 条：backup_database。
//! 导出 .db 全量快照兜底（契约 5.1：与 .cjpack 数据包并存的底线备份）：`VACUUM INTO`。

use rusqlite::Connection;
use tauri::State;

use crate::commands::{file_name_of, resolve_export_path};
use crate::commands::stats::ExportedFile;
use crate::db::Db;
use crate::error::{CommandError, ErrorCode};

/// 执行一次 .db 快照备份（VACUUM INTO）。
/// 失败返回 `E_IO`；数据包导入链路会把该错误包装为 `E_BACKUP`（契约 5.3 规则 7）。
pub(crate) fn perform_backup(
    conn: &Connection,
    db_path: &str,
    target_path: Option<&str>,
) -> Result<ExportedFile, CommandError> {
    let path = resolve_export_path(target_path, db_path, "db", "backup")?;
    // save 对话框语义：用户确认覆盖则先删旧文件（VACUUM INTO 不允许目标已存在）
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| CommandError::new(ErrorCode::Io, format!("覆盖已有备份文件失败：{e}")))?;
    }
    let path_str = path.to_string_lossy().into_owned();
    conn.execute("VACUUM INTO ?1", [&path_str])
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("备份数据库失败：{e}")))?;
    let size_bytes = std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("读取备份文件大小失败：{e}")))?;
    Ok(ExportedFile {
        file_name: file_name_of(&path),
        path: path_str,
        size_bytes,
    })
}

/// 一键备份：VACUUM INTO 生成 .db 快照（target_path 缺省时写入库同级 exports/，契约 4.9）。
#[tauri::command]
pub fn backup_database(
    db: State<'_, Db>,
    target_path: Option<String>,
) -> Result<ExportedFile, CommandError> {
    let conn = db.lock();
    let file = perform_backup(&conn, db.path(), target_path.as_deref())?;
    log::info!("导出 .db 备份：{}（{} 字节）", file.path, file.size_bytes);
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn vacuum_into_produces_readable_snapshot() {
        let db = test_db();
        {
            let conn = db.lock();
            crate::repo::items::create(
                &conn,
                &crate::commands::items::NewItem {
                    name: "备份货".into(),
                    code: None,
                    spec: None,
                    unit: "箱".into(),
                    note: None,
                },
            )
            .unwrap();
        }

        let dir = std::env::temp_dir().join(format!("cangji-bk-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("snap.db");
        let conn = db.lock();
        let out = perform_backup(&conn, db.path(), target.to_str()).unwrap();
        assert_eq!(out.file_name, "snap.db");
        assert!(out.size_bytes > 0);

        // 快照是完整可读的 SQLite 库，且含备份时的数据
        let snap = rusqlite::Connection::open(&out.path).unwrap();
        let n: i64 = snap
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        drop(conn);
        drop(snap);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_validation_rejects_empty_and_wrong_extension() {
        let db = test_db();
        let conn = db.lock();
        let err = perform_backup(&conn, db.path(), Some("   ")).unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid);
        let err = perform_backup(&conn, db.path(), Some("E:\\备份\\a.zip")).unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid);
    }

    #[test]
    fn default_path_falls_back_to_exports_dir() {
        let dir = std::env::temp_dir().join(format!("cangji-bkdef-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::db::Db::open(dir.join("cangji.db")).unwrap();
        let conn = db.lock();
        let out = perform_backup(&conn, db.path(), None).unwrap();
        assert!(out.path.contains("exports"), "{}", out.path);
        assert!(out.file_name.starts_with("cangji-backup-"));
        assert!(out.file_name.ends_with(".db"));
        assert!(out.size_bytes > 0);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
