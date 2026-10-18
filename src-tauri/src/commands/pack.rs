//! 数据包域命令（契约 4.7 / 5 章）。共 3 条：export_data_pack / inspect_data_pack /
//! import_data_pack；另有 `list_merge_logs` 1 条为**契约 4.8 之外的新增命令**
//! （合并日志展示需要，H3 遗留 #3 预告的偏离项，须在 A4 汇报中标注）。
//! .cjpack = 单个 UTF-8 JSON 文本文件（契约 5.2 偏差 #2）；合并算法见契约 5.4。

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{file_name_of, resolve_export_path};
use crate::commands::stats::ExportedFile;
use crate::db::Db;
use crate::error::{CommandError, ErrorCode};
use crate::pack::model::PackFile;

/// 数据包内各类记录条数（契约 5.2 counts）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackCounts {
    pub items: u64,
    pub partners: u64,
    pub operators: u64,
    pub transactions: u64,
}

/// 合并冲突记录（契约 5.4 MergeConflict）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeConflict {
    pub table: String,
    pub id: String,
    pub local_updated_at: String,
    pub incoming_updated_at: String,
    /// "local" | "incoming"
    pub winner: String,
}

/// 导入前预览（契约 5.4 PackSummary）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSummary {
    pub schema_version: u32,
    pub exported_device_id: String,
    pub exported_device_name: String,
    pub exported_at: String,
    pub counts: PackCounts,
    pub checksum_ok: bool,
}

/// 导入合并报告（契约 5.4 ImportReport）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub backup_path: String,
    pub added_transactions: i64,
    pub updated_items: i64,
    pub updated_partners: i64,
    pub new_operators: i64,
    pub conflicts: Vec<MergeConflict>,
}

/// 合并日志行（数据共账页合并日志区；**新增 DTO，契约 4 章无对应命令**——
/// 读 merge_logs 表，契约 3.1 的审计表落地上屏）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeLogEntry {
    pub id: String,
    pub imported_at: String,
    pub source_device_id: String,
    pub source_device_name: Option<String>,
    pub source_file: String,
    pub backup_path: String,
    pub added_transactions: i64,
    pub updated_items: i64,
    pub updated_partners: i64,
    pub new_operators: i64,
    pub conflicts: Vec<MergeConflict>,
}

/// 读源文件字节（空路径 → E_PATH_INVALID，读失败 → E_IO，契约 7.5）。
fn read_pack_bytes(source_path: &str) -> Result<Vec<u8>, CommandError> {
    let trimmed = source_path.trim();
    if trimmed.is_empty() {
        return Err(CommandError::new(ErrorCode::PathInvalid, "数据包路径不能为空"));
    }
    std::fs::read(trimmed)
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("读取数据包失败：{e}")))
}

/// 导出全量数据包（四表全量含已归档；命名 cangji-pack-YYYYMMDD-HHmmss.cjpack，
/// 设备本地时间，契约 5.2）。逻辑体（命令薄封装调用，便于单测）。
pub(crate) fn export_pack(db: &Db, target_path: Option<&str>) -> Result<ExportedFile, CommandError> {
    let conn = db.lock();
    let device_id = crate::device::device_id(&conn)?;
    let device_name = crate::device::device_name(&conn)?;
    let pack = PackFile::build(&conn, env!("CARGO_PKG_VERSION"), &device_id, &device_name)?.seal()?;
    drop(conn);

    let json = serde_json::to_string_pretty(&pack)
        .map_err(|e| CommandError::new(ErrorCode::Internal, format!("数据包序列化失败：{e}")))?;
    let path = resolve_export_path(target_path, db.path(), "cjpack", "pack")?;
    std::fs::write(&path, json)
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("写入数据包失败：{e}")))?;
    let size_bytes = std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| CommandError::new(ErrorCode::Io, format!("读取数据包大小失败：{e}")))?;

    log::info!(
        "导出数据包：{}（items {} / partners {} / operators {} / transactions {}）",
        path.display(),
        pack.counts.items,
        pack.counts.partners,
        pack.counts.operators,
        pack.counts.transactions
    );
    Ok(ExportedFile {
        file_name: file_name_of(&path),
        path: path.to_string_lossy().into_owned(),
        size_bytes,
    })
}

/// 导入前预览：解析 + 校验（契约 5.3 规则 1–6）+ 摘要，不改库。
pub(crate) fn inspect_pack(db: &Db, source_path: &str) -> Result<PackSummary, CommandError> {
    let bytes = read_pack_bytes(source_path)?;
    let pack = PackFile::parse(&bytes)?;
    let conn = db.lock();
    pack.validate(&conn)?;
    Ok(PackSummary {
        schema_version: pack.schema_version,
        exported_device_id: pack.exported_device_id.clone(),
        exported_device_name: pack.exported_device_name.clone(),
        exported_at: pack.exported_at.clone(),
        counts: pack.counts.clone(),
        checksum_ok: pack.verify_checksum()?,
    })
}

/// 导入合并：校验（规则 1–6）→ 自动 backup_database（规则 7，失败 E_BACKUP 拒绝）
/// → 单事务合并（repo/merge.rs，任一步失败整体 ROLLBACK）→ 追加 merge_logs。
pub(crate) fn import_pack(db: &Db, source_path: &str) -> Result<ImportReport, CommandError> {
    let bytes = read_pack_bytes(source_path)?;
    let pack = PackFile::parse(&bytes)?;

    let mut conn = db.lock();
    // 规则 1–6：任一失败即拒绝，不改库
    pack.validate(&conn)?;
    // 规则 7：校验通过后、合并前自动备份（失败 → E_BACKUP，拒绝导入，规划书风险 #4）
    let backup = crate::commands::backup::perform_backup(&conn, db.path(), None).map_err(|e| {
        CommandError::new(ErrorCode::Backup, format!("导入前自动备份失败：{}", e.message))
    })?;
    // merge_logs.source_file 记文件名（审计展示用）
    let source_file = file_name_of(Path::new(source_path.trim()));

    let report = crate::repo::merge::merge_pack(&mut conn, &pack, &source_file, &backup.path)?;
    log::info!(
        "导入数据包合并完成：{}（新增流水 {}、更新货物 {}、更新单位 {}、新增操作人 {}、冲突 {}）",
        source_file,
        report.added_transactions,
        report.updated_items,
        report.updated_partners,
        report.new_operators,
        report.conflicts.len()
    );
    Ok(report)
}

/// 合并日志列表（**新增命令**：契约 4.8 的 24 条之外，供数据共账页展示审计明细）。
pub(crate) fn merge_logs(db: &Db) -> Result<Vec<MergeLogEntry>, CommandError> {
    let conn = db.lock();
    let mut stmt = conn
        .prepare(
            "SELECT id, imported_at, source_device_id, source_device_name, source_file,
                    backup_path, added_transactions, updated_items, updated_partners,
                    new_operators, conflicts_json
             FROM merge_logs ORDER BY imported_at DESC, id DESC",
        )
        .map_err(|e| crate::repo::map_db_err(&e, "查询合并日志"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, String>(10)?,
            ))
        })
        .map_err(|e| crate::repo::map_db_err(&e, "查询合并日志"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| crate::repo::map_db_err(&e, "查询合并日志"))?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                imported_at,
                source_device_id,
                source_device_name,
                source_file,
                backup_path,
                added_transactions,
                updated_items,
                updated_partners,
                new_operators,
                conflicts_json,
            )| {
                // 审计字段自身损坏不阻断列表展示：坏 JSON 降级为空数组
                let conflicts = serde_json::from_str(&conflicts_json).unwrap_or_default();
                MergeLogEntry {
                    id,
                    imported_at,
                    source_device_id,
                    source_device_name,
                    source_file,
                    backup_path,
                    added_transactions,
                    updated_items,
                    updated_partners,
                    new_operators,
                    conflicts,
                }
            },
        )
        .collect())
}

/// 导出全量数据包（契约 4.7/5 章）。
#[tauri::command]
pub fn export_data_pack(
    db: State<'_, Db>,
    target_path: Option<String>,
) -> Result<ExportedFile, CommandError> {
    export_pack(&db, target_path.as_deref())
}

/// 导入前预览：校验 + 摘要，不改库（契约 4.7/5.3）。
#[tauri::command]
pub fn inspect_data_pack(
    db: State<'_, Db>,
    source_path: String,
) -> Result<PackSummary, CommandError> {
    inspect_pack(&db, &source_path)
}

/// 导入合并（契约 4.7/5.4）：校验 → 自动备份 → 单事务合并 → merge_logs 审计。
#[tauri::command]
pub fn import_data_pack(
    db: State<'_, Db>,
    source_path: String,
) -> Result<ImportReport, CommandError> {
    import_pack(&db, &source_path)
}

/// 合并日志列表（**契约偏离**：4.8 核对单之外新增的第 25 条命令）。
#[tauri::command]
pub fn list_merge_logs(db: State<'_, Db>) -> Result<Vec<MergeLogEntry>, CommandError> {
    merge_logs(&db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::partners::NewPartner;
    use crate::commands::transactions::{NewTransaction, TxType};
    use crate::db::test_db;

    /// 源库：1 货物 + 1 单位 + 1 操作人 + 1 流水；返回库与导出的包文件路径。
    fn make_source_pack(dir: &Path) -> (Db, std::path::PathBuf) {
        let db = test_db();
        {
            let conn = db.lock();
            let item = crate::repo::items::create(
                &conn,
                &NewItem { name: "螺丝".into(), code: None, spec: None, unit: "个".into(), note: None },
            )
            .unwrap();
            let partner = crate::repo::partners::create(
                &conn,
                &NewPartner { name: "老王批发部".into(), r#type: "supplier".into(), contact: None, note: None },
            )
            .unwrap();
            let op = crate::repo::operators::create(&conn, "张三").unwrap();
            crate::repo::transactions::create(
                &conn,
                &NewTransaction {
                    tx_type: TxType::In,
                    item_id: item.id,
                    quantity: 50.0,
                    partner_id: Some(partner.id),
                    operator_id: op.id,
                    remark: None,
                    happened_at: Some("2026-03-01T02:00:00.000Z".into()),
                    copied_from: None,
                },
                "dev-src",
            )
            .unwrap();
        }
        let pack_path = dir.join("src-pack.cjpack");
        let file = export_pack(&db, pack_path.to_str()).unwrap();
        assert_eq!(file.file_name, "src-pack.cjpack");
        assert!(file.size_bytes > 0);
        (db, pack_path)
    }

    #[test]
    fn export_inspect_import_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cangji-pack-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let (_src, pack_path) = make_source_pack(&dir);

        // 目标库（真实文件库，便于验证自动备份落点）
        let target = crate::db::Db::open(dir.join("cangji.db")).unwrap();

        // 规则 1–6 预览：不改库
        let summary = inspect_pack(&target, pack_path.to_str().unwrap()).unwrap();
        assert_eq!(summary.schema_version, 1);
        assert!(summary.checksum_ok);
        assert_eq!(summary.counts.items, 1);
        assert_eq!(summary.counts.transactions, 1);
        assert!(!summary.exported_device_id.is_empty());
        {
            let conn = target.lock();
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "inspect 不得改库");
        }

        // 导入：自动备份 + 合并 + merge_logs
        let report = import_pack(&target, pack_path.to_str().unwrap()).unwrap();
        assert_eq!(report.added_transactions, 1);
        assert_eq!(report.updated_items, 1);
        assert_eq!(report.updated_partners, 1);
        assert_eq!(report.new_operators, 1);
        assert!(report.conflicts.is_empty(), "目标库为空不应有冲突");
        assert!(report.backup_path.ends_with(".db"));
        assert!(Path::new(&report.backup_path).exists(), "导入前自动备份文件必须存在");

        {
            let conn = target.lock();
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 1);
            let logs: i64 = conn.query_row("SELECT COUNT(*) FROM merge_logs", [], |r| r.get(0)).unwrap();
            assert_eq!(logs, 1);
            let backup_in_log: String = conn
                .query_row("SELECT backup_path FROM merge_logs", [], |r| r.get(0))
                .unwrap();
            assert_eq!(backup_in_log, report.backup_path);
        }

        // 幂等：再次导入同一包 → 全零增量、无冲突，日志追加第二条
        let report2 = import_pack(&target, pack_path.to_str().unwrap()).unwrap();
        assert_eq!(report2.added_transactions, 0);
        assert_eq!(report2.updated_items, 0);
        assert_eq!(report2.updated_partners, 0);
        assert_eq!(report2.new_operators, 0);
        assert!(report2.conflicts.is_empty());
        {
            let conn = target.lock();
            let n: i64 = conn.query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 1, "幂等去重");
        }

        // 合并日志列表（新增命令逻辑）
        let logs = merge_logs(&target).unwrap();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].imported_at >= logs[1].imported_at, "按时间倒序");
        assert!(logs[0].conflicts.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inspect_rejects_bad_checksum_and_bad_extension_paths() {
        let dir = std::env::temp_dir().join(format!("cangji-pack2-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let (_src, pack_path) = make_source_pack(&dir);
        let target = test_db();

        // 篡改文件内容 → E_CHECKSUM
        let raw = std::fs::read_to_string(&pack_path).unwrap();
        let tampered = raw.replace("螺丝", "螺栓");
        let bad_path = dir.join("bad.cjpack");
        std::fs::write(&bad_path, tampered).unwrap();
        let err = inspect_pack(&target, bad_path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, ErrorCode::Checksum);

        // 空路径 → E_PATH_INVALID；不存在文件 → E_IO
        let err = inspect_pack(&target, "  ").unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid);
        let err = inspect_pack(&target, dir.join("nope.cjpack").to_str().unwrap()).unwrap_err();
        assert_eq!(err.code, ErrorCode::Io);

        // 导出路径扩展名校验 → E_PATH_INVALID
        let err = export_pack(&target, Some("E:\\tmp\\a.zip")).unwrap_err();
        assert_eq!(err.code, ErrorCode::PathInvalid);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
