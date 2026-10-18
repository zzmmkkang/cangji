//! 流水仓储（契约 3.1 transactions 表 + 4.4 append-only 硬约束）。
//! 本文件只包含 INSERT 与 SELECT 两类语句：作废 = 追加 VOID 单（ref_id 指原单），
//! 已落库的行永不改写、永不移除（契约 3.3 偏差 #1、7.4：出库不做库存拦截）。

use rusqlite::{params, params_from_iter, Connection, ToSql};
use uuid::Uuid;

use crate::commands::transactions::{NewTransaction, Transaction, TransactionFilter, TransactionPage, TxType};
use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};

use super::{map_db_err, norm_opt, normalize_rfc3339};

/// v_transaction_detail 视图列（契约 3.1）。
const COLS: &str = "id, type, quantity, remark, happened_at, device_id, copied_from, ref_id, \
                    created_at, updated_at, item_id, item_name, unit, partner_id, partner_name, \
                    operator_id, operator_name";

fn row_to_tx(row: &rusqlite::Row<'_>) -> rusqlite::Result<Transaction> {
    let type_raw: String = row.get(1)?;
    let tx_type = match type_raw.as_str() {
        "IN" => TxType::In,
        "OUT" => TxType::Out,
        "VOID" => TxType::Void,
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                1,
                "type".to_string(),
                rusqlite::types::Type::Text,
            ))
        }
    };
    Ok(Transaction {
        id: row.get(0)?,
        r#type: tx_type,
        item_id: row.get(10)?,
        item_name: row.get(11)?,
        unit: row.get(12)?,
        quantity: row.get(2)?,
        partner_id: row.get(13)?,
        partner_name: row.get(14)?,
        operator_id: row.get(15)?,
        operator_name: row.get(16)?,
        device_id: row.get(5)?,
        remark: row.get(3)?,
        happened_at: row.get(4)?,
        copied_from: row.get(6)?,
        ref_id: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn tx_type_str(t: TxType) -> &'static str {
    match t {
        TxType::In => "IN",
        TxType::Out => "OUT",
        TxType::Void => "VOID",
    }
}

/// 数量校验：恒 > 0、有限数、最多两位小数（契约 3.1 CHECK 口径，入参先于 SQL 校验）。
fn validate_quantity(q: f64) -> Result<(), CommandError> {
    if !q.is_finite() || q <= 0.0 {
        return Err(CommandError::new(ErrorCode::Validation, "数量必须大于 0"));
    }
    if (q * 100.0 - (q * 100.0).round()).abs() > 1e-6 {
        return Err(CommandError::new(ErrorCode::Validation, "数量最多保留两位小数"));
    }
    Ok(())
}

fn require_non_empty(value: &str, field: &str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::new(ErrorCode::Validation, format!("{field}不能为空")));
    }
    Ok(())
}

/// 外键存在性校验：缺失 → E_NOT_FOUND（契约 4.4；D2 出库不查库存，直接放行）。
fn require_ref(conn: &Connection, table: &str, id: &str, field: &str) -> Result<(), CommandError> {
    if id.trim().is_empty() {
        return Err(CommandError::new(ErrorCode::Validation, format!("未选择{field}")));
    }
    // table 仅来自本文件内的固定表名调用点，非用户输入
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
    let context = format!("校验{field}");
    let exists: bool = conn
        .query_row(&sql, [id], |r| r.get(0))
        .map_err(|e| map_db_err(&e, &context))?;
    if !exists {
        return Err(CommandError::new(ErrorCode::NotFound, format!("{field}不存在"))
            .with_detail(serde_json::json!({ "field": field, "value": id })));
    }
    Ok(())
}

/// 按 id 取流水明细（基于 v_transaction_detail 视图）；不存在 → E_NOT_FOUND。
pub fn get(conn: &Connection, id: &str) -> Result<Transaction, CommandError> {
    conn.query_row(&format!("SELECT {COLS} FROM v_transaction_detail WHERE id = ?1"), [id], row_to_tx)
        .map_err(|e| map_db_err(&e, "查询流水"))
}

/// 数据包导出用全量流水（契约 5.2：四表全量导出，不分页）。
/// 按 id 升序：UUID v7 时间有序，被作废原单/被复制单先于其 VOID/复制单，
/// 为合并侧的外键插入顺序提供保障（契约 5.4）。
pub fn all(conn: &Connection) -> Result<Vec<Transaction>, CommandError> {
    let sql = format!("SELECT {COLS} FROM v_transaction_detail ORDER BY id ASC");
    let mut stmt = conn.prepare(&sql).map_err(|e| map_db_err(&e, "查询全量流水"))?;
    let rows = stmt
        .query_map([], row_to_tx)
        .map_err(|e| map_db_err(&e, "查询全量流水"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "查询全量流水"))?;
    Ok(rows)
}

/// 记一笔：追加一条 IN/OUT 流水（VOID 单只能经 `void` 追加）。
pub fn create(conn: &Connection, input: &NewTransaction, device_id: &str) -> Result<Transaction, CommandError> {
    let type_str = match input.tx_type {
        TxType::In => "IN",
        TxType::Out => "OUT",
        TxType::Void => {
            return Err(CommandError::new(
                ErrorCode::Validation,
                "记一笔仅支持入库/出库；作废请调用作废操作",
            ));
        }
    };
    validate_quantity(input.quantity)?;
    require_non_empty(device_id, "设备标识")?;

    require_ref(conn, "items", &input.item_id, "货物")?;
    require_ref(conn, "operators", &input.operator_id, "操作人")?;
    let partner_id = norm_opt(input.partner_id.clone());
    if let Some(pid) = &partner_id {
        require_ref(conn, "partners", pid, "往来单位")?;
    }
    let copied_from = norm_opt(input.copied_from.clone());
    if let Some(cid) = &copied_from {
        require_ref(conn, "transactions", cid, "复制来源单据")?;
    }

    let now = now_rfc3339();
    let happened_at = match &input.happened_at {
        Some(s) => normalize_rfc3339(s, "业务时间")?,
        None => now.clone(),
    };
    let remark = norm_opt(input.remark.clone());

    let id = Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO transactions (id, type, item_id, quantity, partner_id, operator_id, \
         device_id, remark, happened_at, copied_from, ref_id, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, ?11)",
        params![
            id,
            type_str,
            input.item_id,
            input.quantity,
            partner_id,
            input.operator_id,
            device_id,
            remark,
            happened_at,
            copied_from,
            now
        ],
    )
    .map_err(|e| map_db_err(&e, "记一笔"))?;
    get(conn, &id)
}

/// 查某原单已存在的作废单 id（无则 None）。
fn find_void_of(conn: &Connection, ref_id: &str) -> Result<Option<String>, CommandError> {
    match conn.query_row(
        "SELECT id FROM transactions WHERE ref_id = ?1 AND type = 'VOID' LIMIT 1",
        [ref_id],
        |r| r.get::<_, String>(0),
    ) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(map_db_err(&e, "查询作废记录")),
    }
}

/// 作废：追加一条 type='VOID'、quantity=原单数量、ref_id=原单 id 的流水（契约 3.3）。
/// 原单是 VOID → E_VALIDATION；重复作废 → E_VALIDATION（detail 带已有作废单 id）。
pub fn void(conn: &Connection, id: &str, reason: Option<&str>, device_id: &str) -> Result<Transaction, CommandError> {
    let original = get(conn, id)?;
    if matches!(original.r#type, TxType::Void) {
        return Err(CommandError::new(ErrorCode::Validation, "作废单不能再次作废"));
    }
    if let Some(existing_void) = find_void_of(conn, id)? {
        return Err(CommandError::new(ErrorCode::Validation, "该单据已作废，不能重复作废")
            .with_detail(serde_json::json!({ "field": "refId", "voidedTxId": existing_void })));
    }
    require_non_empty(device_id, "设备标识")?;

    let now = now_rfc3339();
    let remark = reason.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let void_id = Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO transactions (id, type, item_id, quantity, partner_id, operator_id, \
         device_id, remark, happened_at, copied_from, ref_id, created_at, updated_at) \
         VALUES (?1, 'VOID', ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?8, ?8)",
        params![
            void_id,
            original.item_id,
            original.quantity,
            original.partner_id,
            original.operator_id,
            device_id,
            remark,
            now,
            id
        ],
    )
    .map_err(|e| map_db_err(&e, "作废单据"))?;
    get(conn, &void_id)
}

/// 流水分页查询：全字段筛选 + keyword（货物名/往来单位名/备注），
/// 按 happened_at DESC 排序，limit 默认 100 上限 500，返回 total（契约 4.4）。
pub fn list(conn: &Connection, filter: &TransactionFilter) -> Result<TransactionPage, CommandError> {
    let limit = filter.limit.unwrap_or(100).clamp(1, 500);
    let offset = filter.offset.unwrap_or(0);

    let date_from = filter
        .date_from
        .as_deref()
        .map(|s| normalize_rfc3339(s, "起始时间"))
        .transpose()?;
    let date_to = filter
        .date_to
        .as_deref()
        .map(|s| normalize_rfc3339(s, "截止时间"))
        .transpose()?;
    if let (Some(from), Some(to)) = (&date_from, &date_to) {
        if from > to {
            return Err(CommandError::new(ErrorCode::Validation, "起始时间不能晚于截止时间"));
        }
    }

    let type_str = filter.tx_type.map(tx_type_str);
    let kw_pattern = filter
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| format!("%{k}%"));

    let mut conds: Vec<String> = Vec::new();
    let mut args: Vec<&dyn ToSql> = Vec::new();
    if let Some(v) = &filter.item_id {
        conds.push("item_id = ?".to_string());
        args.push(v);
    }
    if let Some(v) = &filter.partner_id {
        conds.push("partner_id = ?".to_string());
        args.push(v);
    }
    if let Some(v) = &filter.operator_id {
        conds.push("operator_id = ?".to_string());
        args.push(v);
    }
    if let Some(v) = &type_str {
        conds.push("type = ?".to_string());
        args.push(v);
    }
    if let Some(v) = &date_from {
        conds.push("happened_at >= ?".to_string());
        args.push(v);
    }
    if let Some(v) = &date_to {
        conds.push("happened_at <= ?".to_string());
        args.push(v);
    }
    if let Some(pattern) = &kw_pattern {
        conds.push("(item_name LIKE ? OR partner_name LIKE ? OR remark LIKE ?)".to_string());
        args.push(pattern);
        args.push(pattern);
        args.push(pattern);
    }

    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };

    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM v_transaction_detail{where_sql}"),
            params_from_iter(args.iter().copied()),
            |r| r.get(0),
        )
        .map_err(|e| map_db_err(&e, "统计流水"))?;

    let sql = format!(
        "SELECT {COLS} FROM v_transaction_detail{where_sql} \
         ORDER BY happened_at DESC, id DESC LIMIT {limit} OFFSET {offset}"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| map_db_err(&e, "查询流水列表"))?;
    let rows = stmt
        .query_map(params_from_iter(args.iter().copied()), row_to_tx)
        .map_err(|e| map_db_err(&e, "查询流水列表"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "查询流水列表"))?;

    Ok(TransactionPage { rows, total, limit, offset })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::operators::Operator;
    use crate::db::test_db;

    /// append-only 源码级断言：本文件（含测试自身）不得出现改写/移除语句字面量。
    #[test]
    fn source_is_append_only() {
        let src = include_str!("transactions.rs");
        let write_stmt = concat!("UPD", "ATE");
        let remove_stmt = concat!("DEL", "ETE");
        assert!(
            !src.contains(write_stmt),
            "append-only 违规：源码出现 {write_stmt} 语句"
        );
        assert!(
            !src.contains(remove_stmt),
            "append-only 违规：源码出现 {remove_stmt} 语句"
        );
    }

    struct Fixture {
        db: crate::db::Db,
        item_id: String,
        partner_id: String,
        operator_id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let db = test_db();
            let (item_id, partner_id, operator_id) = {
                let conn = db.lock();
                let item = crate::repo::items::create(
                    &conn,
                    &NewItem { name: "苹果汁".into(), code: None, spec: None, unit: "箱".into(), note: None },
                )
                .unwrap();
                let partner = crate::repo::partners::create(
                    &conn,
                    &crate::commands::partners::NewPartner {
                        name: "老王批发部".into(),
                        r#type: "supplier".into(),
                        contact: None,
                        note: None,
                    },
                )
                .unwrap();
                let operator: Operator = crate::repo::operators::create(&conn, "张三").unwrap();
                (item.id, partner.id, operator.id)
            };
            Self {
                db,
                item_id,
                partner_id,
                operator_id,
            }
        }

        fn new_tx(&self, tx_type: TxType, quantity: f64) -> NewTransaction {
            NewTransaction {
                tx_type,
                item_id: self.item_id.clone(),
                quantity,
                partner_id: Some(self.partner_id.clone()),
                operator_id: self.operator_id.clone(),
                remark: None,
                happened_at: None,
                copied_from: None,
            }
        }

        /// 记一笔（用给定业务时间，"2001-01-01"用于排除出"今天"边界）。
        fn insert_at(&self, tx_type: TxType, quantity: f64, happened_at: &str) -> Transaction {
            let conn = self.db.lock();
            let mut input = self.new_tx(tx_type, quantity);
            input.happened_at = Some(happened_at.to_string());
            create(&conn, &input, "test-device").unwrap()
        }
    }

    #[test]
    fn create_validates_type_quantity_and_refs() {
        let f = Fixture::new();
        let conn = f.db.lock();

        // quantity <= 0 → E_VALIDATION
        let mut input = f.new_tx(TxType::In, 0.0);
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::Validation);
        input.quantity = -5.0;
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::Validation);
        // 超过两位小数 → E_VALIDATION
        input.quantity = 1.234;
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::Validation);
        input.quantity = 10.0;

        // type=VOID 不能直接建 → E_VALIDATION
        input.tx_type = TxType::Void;
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::Validation);
        input.tx_type = TxType::Out;

        // 外键缺失 → E_NOT_FOUND
        input.item_id = "missing-item".into();
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::NotFound);
        input.item_id = f.item_id.clone();
        input.partner_id = Some("missing-partner".into());
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::NotFound);
        input.partner_id = Some(f.partner_id.clone());
        input.operator_id = "missing-operator".into();
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::NotFound);
        input.operator_id = f.operator_id.clone();

        // 合法创建：OUT 不查库存（D2 放行）
        let tx = create(&conn, &input, "dev").unwrap();
        assert_eq!(tx.r#type, TxType::Out);
        assert_eq!(tx.device_id, "dev");
        assert_eq!(tx.created_at.len(), 24);
        assert!(tx.created_at.ends_with('Z'));
        assert!(tx.id < Uuid::now_v7().to_string(), "v7 主键时间有序");
    }

    #[test]
    fn negative_stock_is_allowed_and_void_reverses_in() {
        let f = Fixture::new();

        // IN 10 → OUT 15：库存 -5，出库不拦截（D2）
        f.insert_at(TxType::In, 10.0, "2026-01-05T02:00:00.000Z");
        let out = f.insert_at(TxType::Out, 15.0, "2026-01-05T03:00:00.000Z");
        {
            let conn = f.db.lock();
            let inv = crate::repo::inventory::list(&conn).unwrap();
            let row = inv.iter().find(|r| r.item_id == f.item_id).unwrap();
            assert_eq!(row.total_in, 10.0);
            assert_eq!(row.total_out, 15.0);
            assert_eq!(row.balance, -5.0, "负库存放行（D2）");
            assert!(row.negative);

            // 作废 IN 单：作废方向与原单相反 → −10，即 10 − 15 − 10 = −15（回冲正确）
            // （此场景 VOID 全为 IN 单，与 ΣIN − ΣOUT − ΣVOID 数值一致）
            let in_tx = conn
                .query_row(
                    "SELECT id FROM v_transaction_detail WHERE type = 'IN'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .unwrap();
            let void_tx = void(&conn, &in_tx, Some("记错了"), "dev").unwrap();
            assert_eq!(void_tx.r#type, TxType::Void);
            assert_eq!(void_tx.quantity, 10.0, "VOID 单存原单数量");
            assert_eq!(void_tx.ref_id.as_deref(), Some(in_tx.as_str()));
            assert_eq!(void_tx.item_id, f.item_id);
            assert_eq!(void_tx.remark.as_deref(), Some("记错了"));

            let inv = crate::repo::inventory::list(&conn).unwrap();
            let row = inv.iter().find(|r| r.item_id == f.item_id).unwrap();
            assert_eq!(row.total_voided, 10.0);
            assert_eq!(row.balance, -15.0);
        }

        // 重复作废 → E_VALIDATION；作废单再作废 → E_VALIDATION；未知 id → E_NOT_FOUND
        let conn = f.db.lock();
        let in_tx: String = conn
            .query_row("SELECT id FROM v_transaction_detail WHERE type = 'IN'", [], |r| r.get(0))
            .unwrap();
        let err = void(&conn, &in_tx, None, "dev").unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
        assert!(err.detail.as_ref().unwrap()["voidedTxId"].is_string());

        let void_id: String = conn
            .query_row("SELECT id FROM v_transaction_detail WHERE type = 'VOID'", [], |r| r.get(0))
            .unwrap();
        let err = void(&conn, &void_id, None, "dev").unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);

        let err = void(&conn, "no-such-id", None, "dev").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);

        // 作废 OUT 单：回冲方向按被作废原单类型（契约修订 v1.1）——
        // balance = ΣIN − ΣOUT + ΣVOID(原单OUT) − ΣVOID(原单IN)
        // = 10 − 15 + 15 − 10 = 0（IN 10 与 OUT 15 均已作废，全部回冲归零）
        let void_out = void(&conn, &out.id, Some("客户取消"), "dev").unwrap();
        assert_eq!(void_out.quantity, out.quantity);
        drop(conn);
        let conn = f.db.lock();
        let inv = crate::repo::inventory::list(&conn).unwrap();
        let row = inv.iter().find(|r| r.item_id == f.item_id).unwrap();
        assert_eq!(row.total_voided, 25.0);
        assert_eq!(row.balance, 0.0, "作废 OUT 单回冲 +15：双向回冲语义（契约修订 v1.1）");
    }

    #[test]
    fn list_filters_pagination_and_keyword() {
        let f = Fixture::new();
        f.insert_at(TxType::In, 10.0, "2026-01-05T02:00:00.000Z");
        f.insert_at(TxType::Out, 3.0, "2026-01-06T02:00:00.000Z");
        let tx = f.insert_at(TxType::Out, 1.0, "2026-01-07T02:00:00.000Z");
        {
            let conn = f.db.lock();
            let mut input = f.new_tx(TxType::In, 2.0);
            input.remark = Some("周末加单".into());
            input.happened_at = Some("2026-01-08T02:00:00.000Z".into());
            create(&conn, &input, "dev").unwrap();
        }

        let conn = f.db.lock();
        // 分页 + total
        let page = list(&conn, &TransactionFilter { limit: Some(2), ..Default::default() }).unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.total, 4);
        assert_eq!(page.limit, 2);
        assert_eq!(page.offset, 0);
        // 默认按 happened_at DESC
        assert!(page.rows[0].happened_at >= page.rows[1].happened_at);

        let page2 = list(
            &conn,
            &TransactionFilter { limit: Some(2), offset: Some(2), ..Default::default() },
        )
        .unwrap();
        assert_eq!(page2.rows.len(), 2);
        assert_eq!(page2.total, 4);

        // limit 越界收敛：0 → 1，9999 → 500
        let page = list(&conn, &TransactionFilter { limit: Some(0), ..Default::default() }).unwrap();
        assert_eq!(page.limit, 1);
        let page = list(&conn, &TransactionFilter { limit: Some(9999), ..Default::default() }).unwrap();
        assert_eq!(page.limit, 500);

        // type 筛选
        let page = list(
            &conn,
            &TransactionFilter { tx_type: Some(TxType::In), ..Default::default() },
        )
        .unwrap();
        assert_eq!(page.total, 2);

        // keyword：货物名
        let page = list(
            &conn,
            &TransactionFilter { keyword: Some("苹果".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(page.total, 4, "货物名匹配全部单据");
        // keyword：备注
        let page = list(
            &conn,
            &TransactionFilter { keyword: Some("周末".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(page.total, 1);
        // keyword：往来单位名
        let page = list(
            &conn,
            &TransactionFilter { keyword: Some("老王".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(page.total, 4);

        // 时间段筛选：只落在 01-07 当天的单据
        let page = list(
            &conn,
            &TransactionFilter {
                date_from: Some("2026-01-07T00:00:00.000Z".into()),
                date_to: Some("2026-01-07T23:59:59.999Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].id, tx.id);

        // 起止边界均为闭区间：happened_at 恰等于起/止时刻仍命中
        let page = list(
            &conn,
            &TransactionFilter {
                date_from: Some("2026-01-06T02:00:00.000Z".into()),
                date_to: Some("2026-01-06T02:00:00.000Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].quantity, 3.0);

        // 非法时间 → E_VALIDATION；起止颠倒 → E_VALIDATION
        let err = list(
            &conn,
            &TransactionFilter { date_from: Some("2026-01-06".into()), ..Default::default() },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
        let err = list(
            &conn,
            &TransactionFilter {
                date_from: Some("2026-01-08T00:00:00.000Z".into()),
                date_to: Some("2026-01-06T00:00:00.000Z".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);

        // 筛选维度：操作人
        let page = list(
            &conn,
            &TransactionFilter { operator_id: Some(f.operator_id.clone()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(page.total, 4);
    }

    #[test]
    fn happened_at_is_normalized_to_utc_millis() {
        let f = Fixture::new();
        let conn = f.db.lock();
        let mut input = f.new_tx(TxType::In, 5.0);
        // 前端若传带 +08:00 偏移的时间，落库统一归一化为 UTC 毫秒（契约 7.3）
        input.happened_at = Some("2026-10-17T08:00:00+08:00".into());
        let tx = create(&conn, &input, "dev").unwrap();
        assert_eq!(tx.happened_at, "2026-10-17T00:00:00.000Z");

        input.happened_at = Some("不是时间".into());
        assert_eq!(create(&conn, &input, "dev").unwrap_err().code, ErrorCode::Validation);
    }
}
