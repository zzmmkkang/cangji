//! 数据包合并（契约 5.4）：单事务（BEGIN IMMEDIATE）内完成 operators 按 name 归并重映射、
//! items/partners UUID 幂等 + LWW（包内 `updated_at` **严格大于**本库才覆盖，相等视为本库新）、
//! transactions UUID 幂等去重、merge_logs 追加审计。任一步失败整体 ROLLBACK，本库保持原状。
//! 合并前的自动备份由导入命令负责（契约 5.3 规则 7），合并结果计数回写 ImportReport 与 merge_logs。

use std::collections::HashMap;

use rusqlite::{params, Connection, TransactionBehavior};
use uuid::Uuid;

use crate::commands::pack::{ImportReport, MergeConflict};
use crate::commands::transactions::Transaction;
use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};
use crate::pack::model::PackFile;

use super::map_db_err;

/// 距上次同步：merge_logs 最大 imported_at（契约 4.1 AppInfo.last_sync_at；无导入记录为 None）。
pub fn last_sync_at(conn: &Connection) -> Result<Option<String>, CommandError> {
    conn.query_row("SELECT MAX(imported_at) FROM merge_logs", [], |r| {
        r.get::<_, Option<String>>(0)
    })
    .map_err(|e| map_db_err(&e, "查询上次同步时间"))
}

/// 按 id 查本库流水原始行（合并期内容比对用；None = 本库无此 id）。
type LocalTx = (
    String,          // type
    f64,             // quantity
    String,          // item_id
    Option<String>,  // partner_id
    String,          // operator_id
    String,          // device_id
    Option<String>,  // remark
    String,          // happened_at
    Option<String>,  // copied_from
    Option<String>,  // ref_id
    String,          // created_at
    String,          // updated_at
);

fn local_tx(conn: &Connection, id: &str) -> Result<Option<LocalTx>, CommandError> {
    match conn.query_row(
        "SELECT type, quantity, item_id, partner_id, operator_id, device_id, remark,
                happened_at, copied_from, ref_id, created_at, updated_at
         FROM transactions WHERE id = ?1",
        [id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
            ))
        },
    ) {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(map_db_err(&e, "合并前查询流水")),
    }
}

fn tx_type_str(t: crate::commands::transactions::TxType) -> &'static str {
    match t {
        crate::commands::transactions::TxType::In => "IN",
        crate::commands::transactions::TxType::Out => "OUT",
        crate::commands::transactions::TxType::Void => "VOID",
    }
}

/// 是否为外键未就绪错误（合并插入序依赖被引用行先行，可延迟重试）。
fn is_fk_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    )
}

/// LWW 裁决：包内 `updated_at` 严格大于本库才覆盖（契约 5.4；UTC 毫秒字符串序 = 时间序）。
/// 返回 `(是否覆盖, 有无冲突记录)`：时间戳不同即记 conflict（严格大于 → incoming 胜；
/// 否则本库新 → local 胜）；完全相等视为已同步，跳过且不记冲突（保证重复导入幂等干净）。
fn lww_wins(local_updated_at: &str, incoming_updated_at: &str) -> (bool, Option<&'static str>) {
    if incoming_updated_at > local_updated_at {
        (true, Some("incoming"))
    } else if incoming_updated_at < local_updated_at {
        (false, Some("local"))
    } else {
        (false, None)
    }
}

/// 数据包合并入口（契约 5.4）：校验通过的包在单个 `BEGIN IMMEDIATE` 事务内合并，
/// 任一步失败整体 ROLLBACK；完成后追加一条 merge_logs 审计。
/// `source_file` 为包文件名，`backup_path` 为规则 7 已完成的自动备份路径。
pub fn merge_pack(
    conn: &mut Connection,
    pack: &PackFile,
    source_file: &str,
    backup_path: &str,
) -> Result<ImportReport, CommandError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| map_db_err(&e, "开启合并事务"))?;

    let mut conflicts: Vec<MergeConflict> = Vec::new();
    let mut new_operators: i64 = 0;
    let mut updated_items: i64 = 0;
    let mut updated_partners: i64 = 0;
    let mut added_transactions: i64 = 0;

    // ── 1) operators：按 name 归并，包内 operator_id 重映射到本库 id ──────────────
    let mut op_remap: HashMap<String, String> = HashMap::new();
    for op in &pack.data.operators {
        let by_name: Option<String> = match tx.query_row(
            "SELECT id FROM operators WHERE name = ?1",
            [op.name.as_str()],
            |r| r.get(0),
        ) {
            Ok(id) => Some(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(map_db_err(&e, "合并操作人")),
        };
        if let Some(local_id) = by_name {
            // 本库已有同名 → id 以本库为准，包内流水的 operator_id 重映射
            op_remap.insert(op.id.clone(), local_id);
            continue;
        }
        // id 撞车但名字不同（UUID 碰撞，理论不发生）→ 以本库为准，仍重映射
        let by_id: Option<String> = match tx.query_row(
            "SELECT id FROM operators WHERE id = ?1",
            [op.id.as_str()],
            |r| r.get::<_, String>(0),
        ) {
            Ok(id) => Some(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(map_db_err(&e, "合并操作人")),
        };
        if by_id.is_some() {
            op_remap.insert(op.id.clone(), op.id.clone());
            continue;
        }
        tx.execute(
            "INSERT INTO operators (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![op.id, op.name, op.created_at, op.updated_at],
        )
        .map_err(|e| map_db_err(&e, "合并操作人"))?;
        op_remap.insert(op.id.clone(), op.id.clone());
        new_operators += 1;
    }

    // ── 2) items：UUID 幂等 + LWW（覆盖含 is_archived，整行覆盖） ─────────────────
    for item in &pack.data.items {
        let local_updated: Option<String> = match tx.query_row(
            "SELECT updated_at FROM items WHERE id = ?1",
            [item.id.as_str()],
            |r| r.get(0),
        ) {
            Ok(ts) => Some(ts),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(map_db_err(&e, "合并货物档案")),
        };
        match local_updated {
            None => {
                tx.execute(
                    "INSERT INTO items (id, name, code, spec, unit, note, is_archived, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        item.id,
                        item.name,
                        item.code,
                        item.spec,
                        item.unit,
                        item.note,
                        item.is_archived as i64,
                        item.created_at,
                        item.updated_at
                    ],
                )
                .map_err(|e| map_db_err(&e, "合并货物档案"))?;
                updated_items += 1;
            }
            Some(local_ts) => {
                let (win, winner) = lww_wins(&local_ts, &item.updated_at);
                if win {
                    tx.execute(
                        "UPDATE items SET name = ?1, code = ?2, spec = ?3, unit = ?4, note = ?5,
                                 is_archived = ?6, created_at = ?7, updated_at = ?8
                         WHERE id = ?9",
                        params![
                            item.name,
                            item.code,
                            item.spec,
                            item.unit,
                            item.note,
                            item.is_archived as i64,
                            item.created_at,
                            item.updated_at,
                            item.id
                        ],
                    )
                    .map_err(|e| map_db_err(&e, "合并货物档案"))?;
                    updated_items += 1;
                }
                if let Some(w) = winner {
                    conflicts.push(MergeConflict {
                        table: "items".into(),
                        id: item.id.clone(),
                        local_updated_at: local_ts,
                        incoming_updated_at: item.updated_at.clone(),
                        winner: w.into(),
                    });
                }
            }
        }
    }

    // ── 3) partners：同 items（UUID 幂等 + LWW） ──────────────────────────────────
    for p in &pack.data.partners {
        let local_updated: Option<String> = match tx.query_row(
            "SELECT updated_at FROM partners WHERE id = ?1",
            [p.id.as_str()],
            |r| r.get(0),
        ) {
            Ok(ts) => Some(ts),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(map_db_err(&e, "合并往来单位")),
        };
        match local_updated {
            None => {
                tx.execute(
                    "INSERT INTO partners (id, name, type, contact, note, is_archived, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        p.id,
                        p.name,
                        p.r#type,
                        p.contact,
                        p.note,
                        p.is_archived as i64,
                        p.created_at,
                        p.updated_at
                    ],
                )
                .map_err(|e| map_db_err(&e, "合并往来单位"))?;
                updated_partners += 1;
            }
            Some(local_ts) => {
                let (win, winner) = lww_wins(&local_ts, &p.updated_at);
                if win {
                    tx.execute(
                        "UPDATE partners SET name = ?1, type = ?2, contact = ?3, note = ?4,
                             is_archived = ?5, created_at = ?6, updated_at = ?7
                         WHERE id = ?8",
                        params![
                            p.name,
                            p.r#type,
                            p.contact,
                            p.note,
                            p.is_archived as i64,
                            p.created_at,
                            p.updated_at,
                            p.id
                        ],
                    )
                    .map_err(|e| map_db_err(&e, "合并往来单位"))?;
                    updated_partners += 1;
                }
                if let Some(w) = winner {
                    conflicts.push(MergeConflict {
                        table: "partners".into(),
                        id: p.id.clone(),
                        local_updated_at: local_ts,
                        incoming_updated_at: p.updated_at.clone(),
                        winner: w.into(),
                    });
                }
            }
        }
    }

    // ── 4) transactions：UUID 幂等去重 + operator_id 重映射 ───────────────────────
    // 按 id 升序（UUID v7 时间有序 → 被引用原单先于 VOID/复制单）；
    // 外键仍可能因跨包引用未就绪 → 延迟到下一轮重试，无法推进才报错回滚。
    let mut pending: Vec<&Transaction> = pack.data.transactions.iter().collect();
    pending.sort_by(|a, b| a.id.cmp(&b.id));

    loop {
        let mut progressed = false;
        let mut deferred: Option<rusqlite::Error> = None;
        let mut idx = 0;
        while idx < pending.len() {
            let t = pending[idx];
            match local_tx(&tx, &t.id)? {
                // 已存在 → 幂等跳过；内容不一致以本库为准并记 conflict（正常不应发生）
                Some(local) => {
                    let incoming_op = op_remap
                        .get(&t.operator_id)
                        .cloned()
                        .unwrap_or_else(|| t.operator_id.clone());
                    let differs = local.0 != tx_type_str(t.r#type)
                        || (local.1 - t.quantity).abs() > f64::EPSILON
                        || local.2 != t.item_id
                        || local.3 != t.partner_id
                        || local.4 != incoming_op
                        || local.5 != t.device_id
                        || local.6 != t.remark
                        || local.7 != t.happened_at
                        || local.8 != t.copied_from
                        || local.9 != t.ref_id;
                    if differs {
                        conflicts.push(MergeConflict {
                            table: "transactions".into(),
                            id: t.id.clone(),
                            local_updated_at: local.11.clone(),
                            incoming_updated_at: t.updated_at.clone(),
                            winner: "local".into(),
                        });
                    }
                    pending.remove(idx);
                    progressed = true;
                }
                None => {
                    let operator_id = op_remap
                        .get(&t.operator_id)
                        .cloned()
                        .unwrap_or_else(|| t.operator_id.clone());
                    match tx.execute(
                        "INSERT INTO transactions (id, type, item_id, quantity, partner_id, operator_id,
                                 device_id, remark, happened_at, copied_from, ref_id, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            t.id,
                            tx_type_str(t.r#type),
                            t.item_id,
                            t.quantity,
                            t.partner_id,
                            operator_id,
                            t.device_id,
                            t.remark,
                            t.happened_at,
                            t.copied_from,
                            t.ref_id,
                            t.created_at,
                            t.updated_at
                        ],
                    ) {
                        Ok(_) => {
                            added_transactions += 1;
                            pending.remove(idx);
                            progressed = true;
                        }
                        Err(e) if is_fk_violation(&e) => {
                            deferred = Some(e);
                            idx += 1;
                        }
                        Err(e) => return Err(map_db_err(&e, "合并流水")),
                    }
                }
            }
        }
        if pending.is_empty() {
            break;
        }
        if !progressed {
            // 所有剩余行都卡在外键上（引用既不在包内也未先插入）→ 报错并整体回滚
            let e = deferred.expect("无进展时必有被延迟的外键错误");
            return Err(map_db_err(&e, "合并流水（引用无法解析）"));
        }
    }

    // ── 5) merge_logs 追加审计（同一事务内，回滚则一并不落） ─────────────────────
    let imported_at = now_rfc3339();
    let conflicts_json = serde_json::to_string(&conflicts)
        .map_err(|e| CommandError::new(ErrorCode::Internal, format!("冲突明细序列化失败：{e}")))?;
    tx.execute(
        "INSERT INTO merge_logs (id, imported_at, source_device_id, source_device_name, source_file,
                backup_path, added_transactions, updated_items, updated_partners, new_operators, conflicts_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            Uuid::now_v7().to_string(),
            imported_at,
            pack.exported_device_id,
            pack.exported_device_name,
            source_file,
            backup_path,
            added_transactions,
            updated_items,
            updated_partners,
            new_operators,
            conflicts_json
        ],
    )
    .map_err(|e| map_db_err(&e, "写入合并日志"))?;

    tx.commit().map_err(|e| map_db_err(&e, "提交合并事务"))?;

    Ok(ImportReport {
        backup_path: backup_path.to_string(),
        added_transactions,
        updated_items,
        updated_partners,
        new_operators,
        conflicts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::partners::NewPartner;
    use crate::commands::transactions::{NewTransaction, TxType};
    use crate::db::test_db;
    use crate::error::ErrorCode;
    use crate::pack::model::PackFile;

    fn item(conn: &Connection, name: &str) -> crate::commands::items::Item {
        crate::repo::items::create(
            conn,
            &NewItem { name: name.into(), code: None, spec: None, unit: "箱".into(), note: None },
        )
        .unwrap()
    }

    fn seed(db: &crate::db::Db) {
        let conn = db.lock();
        let i = item(&conn, "苹果汁");
        let p = crate::repo::partners::create(
            &conn,
            &NewPartner { name: "老王批发部".into(), r#type: "supplier".into(), contact: None, note: None },
        )
        .unwrap();
        let op = crate::repo::operators::create(&conn, "张三").unwrap();
        crate::repo::transactions::create(
            &conn,
            &NewTransaction {
                tx_type: TxType::In,
                item_id: i.id,
                quantity: 10.0,
                partner_id: Some(p.id),
                operator_id: op.id,
                remark: None,
                happened_at: Some("2026-01-01T02:00:00.000Z".into()),
                copied_from: None,
            },
            "dev-local",
        )
        .unwrap();
    }

    /// 导出 `db` 的全量包（未封印，便于测试篡改后重封）。
    fn build_pack(db: &crate::db::Db) -> PackFile {
        let conn = db.lock();
        PackFile::build(&conn, "0.1.0", "src-dev", "来源设备").unwrap()
    }

    #[test]
    fn merge_is_idempotent_with_lww_and_operator_remap() {
        // 本库（已有 张三/苹果汁/老王批发部 + 1 笔入库），档案时间调旧供 LWW 对比
        let local = test_db();
        seed(&local);
        {
            let conn = local.lock();
            conn.execute(
                "UPDATE items SET updated_at = '2026-01-01T00:00:00.000Z' WHERE name = '苹果汁'",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE partners SET updated_at = '2026-01-01T00:00:00.000Z' WHERE name = '老王批发部'",
                [],
            )
            .unwrap();
        }

        // "来源设备"的包：同 id 档案但 updated_at 更新（LWW incoming）、
        // 同名操作人不同 id（按 name 归并 + 流水重映射）、新增操作人/货物/流水
        let mut pack = build_pack(&local);
        let remapped_op_id = "01900000-0000-7000-8000-000000000001".to_string();
        let new_op = crate::commands::operators::Operator {
            id: "01900000-0000-7000-8000-000000000002".into(),
            name: "李四".into(),
            created_at: "2026-05-01T00:00:00.000Z".into(),
            updated_at: "2026-05-01T00:00:00.000Z".into(),
        };
        let new_item = crate::commands::items::Item {
            id: "01900000-0000-7000-8000-000000000003".into(),
            name: "香蕉".into(),
            code: None,
            spec: None,
            unit: "箱".into(),
            note: None,
            is_archived: false,
            created_at: "2026-05-01T00:00:00.000Z".into(),
            updated_at: "2026-05-02T00:00:00.000Z".into(),
        };
        {
            // 同名操作人换成"来源设备自己的 id"（同名不同 id → 归并重映射）
            let local_op_id = pack.data.operators[0].id.clone();
            pack.data.operators[0].id = remapped_op_id.clone();
            // 原流水仍指本库 op id（包内不存在，本库可解析 → 合法，内容比对不记冲突）
            // 新增一笔流水：用来源设备的"张三"（应重映射到本库张三 id）
            pack.data.operators.push(new_op);
            pack.data.items.push(new_item);
            pack.counts.operators = pack.data.operators.len() as u64;
            pack.counts.items = pack.data.items.len() as u64;
            // 档案 LWW：incoming 比本库新
            pack.data.items[0].updated_at = "2026-05-05T00:00:00.000Z".into();
            pack.data.items[0].note = Some("远端新备注".into());
            pack.data.partners[0].updated_at = "2026-05-05T00:00:00.000Z".into();
            pack.data.partners[0].contact = Some("13800000000".into());
            pack.data.transactions.push(crate::commands::transactions::Transaction {
                id: "01900000-0000-7000-8000-000000000009".into(),
                r#type: TxType::In,
                item_id: new_item_id(&pack),
                quantity: 7.0,
                partner_id: None,
                operator_id: remapped_op_id.clone(),
                unit: "箱".into(),
                item_name: "香蕉".into(),
                partner_name: None,
                operator_name: "张三".into(),
                device_id: "dev-remote".into(),
                remark: None,
                happened_at: "2026-05-06T02:00:00.000Z".into(),
                copied_from: None,
                ref_id: None,
                created_at: "2026-05-06T02:00:00.000Z".into(),
                updated_at: "2026-05-06T02:00:00.000Z".into(),
            });
            pack.counts.transactions = pack.data.transactions.len() as u64;
            let _ = local_op_id;
        }
        let pack = pack.seal().unwrap();

        let mut conn = local.lock();
        let report = merge_pack(&mut conn, &pack, "remote.cjpack", "/tmp/bk.db").unwrap();

        assert_eq!(report.new_operators, 1, "仅李四为新操作人；张三按 name 归并");
        assert_eq!(report.added_transactions, 1, "新增 1 笔；包内原流水与本库同 id 去重");
        assert_eq!(report.updated_items, 2, "苹果汁 LWW 覆盖 + 香蕉新插");
        assert_eq!(report.updated_partners, 1, "老王批发部 LWW 覆盖");
        // 冲突：苹果汁、老王批发部（incoming 胜）；原流水内容一致不记冲突
        assert_eq!(report.conflicts.len(), 2);
        assert!(report.conflicts.iter().all(|c| c.winner == "incoming"));
        assert!(report.conflicts.iter().any(|c| c.table == "items"));
        assert!(report.conflicts.iter().any(|c| c.table == "partners"));

        // operator 重映射：新流水必须挂到本库张三的 id 上，而非来源设备 id
        let local_zhang: String = conn
            .query_row("SELECT id FROM operators WHERE name = '张三'", [], |r| r.get(0))
            .unwrap();
        assert_ne!(local_zhang, remapped_op_id);
        let remapped_n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE operator_id = ?1 AND device_id = 'dev-remote'",
                [local_zhang.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remapped_n, 1, "来源设备张三的流水已重映射到本库张三");
        let stray: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE operator_id = ?1",
                [remapped_op_id.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stray, 0, "来源设备 operator id 不得落库");

        // LWW 结果落库
        let note: Option<String> = conn
            .query_row("SELECT note FROM items WHERE name = '苹果汁'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(note.as_deref(), Some("远端新备注"));
        let contact: Option<String> = conn
            .query_row("SELECT contact FROM partners WHERE name = '老王批发部'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(contact.as_deref(), Some("13800000000"));

        // merge_logs 落库 + last_sync_at 可读
        let logs: i64 = conn.query_row("SELECT COUNT(*) FROM merge_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(logs, 1);
        assert!(last_sync_at(&conn).unwrap().is_some());

        // 二次导入同一包：全零增量、无冲突（幂等），日志追加
        let report2 = merge_pack(&mut conn, &pack, "remote.cjpack", "/tmp/bk.db").unwrap();
        assert_eq!(report2.added_transactions, 0);
        assert_eq!(report2.updated_items, 0);
        assert_eq!(report2.updated_partners, 0);
        assert_eq!(report2.new_operators, 0);
        assert!(report2.conflicts.is_empty(), "{:?}", report2.conflicts);
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "重复导入不产生重复流水");
        let logs: i64 = conn.query_row("SELECT COUNT(*) FROM merge_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(logs, 2);
    }

    /// 取包内新增货物的 id（供测试构造引用它的流水）。
    fn new_item_id(pack: &PackFile) -> String {
        pack.data
            .items
            .iter()
            .find(|i| i.name == "香蕉")
            .map(|i| i.id.clone())
            .unwrap()
    }

    #[test]
    fn lww_local_wins_and_equal_is_silent() {
        // 本库档案时间调旧 → 供"本库新"裁决
        let local = test_db();
        seed(&local);
        {
            let conn = local.lock();
            conn.execute(
                "UPDATE items SET updated_at = '2026-01-01T00:00:00.000Z' WHERE name = '苹果汁'",
                [],
            )
            .unwrap();
        }

        // 包与本库同 id，但 incoming 更旧 → local 胜、记 conflict、内容不覆盖
        let mut pack = build_pack(&local);
        pack.data.items[0].updated_at = "2020-01-01T00:00:00.000Z".into();
        pack.data.items[0].note = Some("旧备注".into());
        let pack = pack.seal().unwrap();

        let mut conn = local.lock();
        let report = merge_pack(&mut conn, &pack, "r.cjpack", "/tmp/bk.db").unwrap();
        let item_conflict = report
            .conflicts
            .iter()
            .find(|c| c.table == "items")
            .expect("苹果汁应记 conflict");
        assert_eq!(item_conflict.winner, "local");
        let note: Option<String> = conn
            .query_row("SELECT note FROM items WHERE name = '苹果汁'", [], |r| r.get(0))
            .unwrap();
        assert!(note.is_none(), "本库新，覆盖不得发生");
        // 先释放本库锁再导包（std::sync::Mutex 不可重入，同线程再 lock 会死锁）
        drop(conn);

        // 从本库自身导出的原样包（时间戳相等）重导 → 全部跳过且不记冲突（幂等干净）
        let pack2 = build_pack(&local).seal().unwrap();
        let mut conn = local.lock();
        let report2 = merge_pack(&mut conn, &pack2, "r.cjpack", "/tmp/bk.db").unwrap();
        assert!(report2.conflicts.is_empty(), "{:?}", report2.conflicts);
        assert_eq!(report2.added_transactions, 0);
        assert_eq!(report2.updated_items, 0);
        assert_eq!(report2.updated_partners, 0);
        assert_eq!(report2.new_operators, 0);
    }

    #[test]
    fn void_transaction_with_ref_merges_in_order() {
        // 远端含 VOID 单（ref_id 指向包内原单）：按 id 升序插入，外键成立
        let remote = test_db();
        {
            let conn = remote.lock();
            let i = item(&conn, "大米");
            let op = crate::repo::operators::create(&conn, "王五").unwrap();
            let tx = crate::repo::transactions::create(
                &conn,
                &NewTransaction {
                    tx_type: TxType::In,
                    item_id: i.id,
                    quantity: 5.0,
                    partner_id: None,
                    operator_id: op.id,
                    remark: None,
                    happened_at: Some("2026-01-01T02:00:00.000Z".into()),
                    copied_from: None,
                },
                "dev-remote",
            )
            .unwrap();
            crate::repo::transactions::void(&conn, &tx.id, Some("录错了"), "dev-remote").unwrap();
        }
        let pack = build_pack(&remote).seal().unwrap();
        assert_eq!(pack.data.transactions.len(), 2);

        let local = test_db();
        let mut conn = local.lock();
        let report = merge_pack(&mut conn, &pack, "r.cjpack", "/tmp/bk.db").unwrap();
        assert_eq!(report.added_transactions, 2);
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM transactions WHERE type = 'VOID'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let ref_ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM transactions v JOIN transactions o ON o.id = v.ref_id WHERE v.type = 'VOID'",
            [],
            |r| r.get(0),
        )
        .unwrap();
        assert_eq!(ref_ok, 1, "VOID 单引用的原单已随包合入");
    }

    #[test]
    fn failure_rolls_back_everything() {
        // 手造坏包：流水指向不存在货物 → 插入阶段外键无法解析 → 整体 ROLLBACK
        let local = test_db();
        seed(&local);
        let remote = test_db();
        let mut pack = build_pack(&remote).seal().unwrap();
        pack.data.transactions.clear();

        let before: i64;
        {
            let conn = local.lock();
            before = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        }

        // 直接注入一条 item 悬空的流水（绕过 validate，专测 merge 回滚）
        pack.data.transactions.push(crate::commands::transactions::Transaction {
            id: "01990000-0000-7000-8000-000000000009".into(),
            r#type: TxType::In,
            item_id: "01990000-0000-7000-8000-000000000008".into(),
            quantity: 1.0,
            partner_id: None,
            operator_id: "01990000-0000-7000-8000-000000000007".into(),
            unit: "箱".into(),
            item_name: "悬空货".into(),
            partner_name: None,
            operator_name: "悬空人".into(),
            device_id: "dev".into(),
            remark: None,
            happened_at: "2026-01-01T00:00:00.000Z".into(),
            copied_from: None,
            ref_id: None,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            updated_at: "2026-01-01T00:00:00.000Z".into(),
        });
        // counts 同步（否则 validate 层面不一致——merge 不做校验，仅为自洽）
        pack.counts.transactions = pack.data.transactions.len() as u64;

        let mut conn = local.lock();
        let err = merge_pack(&mut conn, &pack, "bad.cjpack", "/tmp/bk.db").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound, "{}", err.message);

        // 全量回滚：items 未变、merge_logs 未写
        let items: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(items, before, "回滚后本库保持原状");
        let logs: i64 = conn.query_row("SELECT COUNT(*) FROM merge_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(logs, 0, "失败时不得写入 merge_logs");
        let txs: i64 = conn.query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(txs, 1, "原有流水不受影响");
    }
}
