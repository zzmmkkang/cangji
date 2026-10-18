//! 统计查询（契约 4.6：按期间 + 维度聚合进出量）。
//! 维度：item（按货物）/ partner（按往来单位）；另支持 operator（按操作人，见偏差说明）。

use rusqlite::{params_from_iter, Connection, ToSql};

use crate::commands::stats::{StatsQuery, StatsRow};
use crate::error::{CommandError, ErrorCode};

use super::{map_db_err, normalize_rfc3339};

/// 合法维度：契约 4.6 定义 item | partner；operator 为超集扩展（按操作人汇总，任务要求）。
const GROUP_DIMS: [&str; 3] = ["item", "partner", "operator"];

/// 按维度聚合期间进出台账：in_qty/out_qty 按单据方向累计，tx_count 计 IN/OUT，
/// void_count 计 VOID；未指定往来单位的单据归入「未指定」组（group_key 空串）。
pub fn query(conn: &Connection, q: &StatsQuery) -> Result<Vec<StatsRow>, CommandError> {
    if !GROUP_DIMS.contains(&q.group_by.as_str()) {
        return Err(CommandError::new(
            ErrorCode::Validation,
            "统计维度必须为 item / partner / operator",
        )
        .with_detail(serde_json::json!({ "field": "groupBy", "value": q.group_by })));
    }
    let date_from = normalize_rfc3339(&q.date_from, "起始时间")?;
    let date_to = normalize_rfc3339(&q.date_to, "截止时间")?;
    if date_from > date_to {
        return Err(CommandError::new(ErrorCode::Validation, "起始时间不能晚于截止时间"));
    }

    let (select_from, group_col, name_expr) = match q.group_by.as_str() {
        "partner" => (
            "transactions t LEFT JOIN partners p ON p.id = t.partner_id",
            "t.partner_id",
            "COALESCE(p.name, '未指定')",
        ),
        "operator" => (
            "transactions t JOIN operators o ON o.id = t.operator_id",
            "t.operator_id",
            "o.name",
        ),
        _ => (
            "transactions t JOIN items i ON i.id = t.item_id",
            "t.item_id",
            "i.name",
        ),
    };

    let mut conds = vec![
        "t.happened_at >= ?".to_string(),
        "t.happened_at <= ?".to_string(),
    ];
    let mut args: Vec<&dyn ToSql> = vec![&date_from, &date_to];
    if let Some(item_id) = &q.item_id {
        conds.push("t.item_id = ?".to_string());
        args.push(item_id);
    }
    if let Some(partner_id) = &q.partner_id {
        conds.push("t.partner_id = ?".to_string());
        args.push(partner_id);
    }

    let sql = format!(
        "SELECT COALESCE({group_col}, '') AS group_key, {name_expr} AS group_name, \
                COALESCE(SUM(CASE WHEN t.type = 'IN'   THEN t.quantity END), 0) AS in_qty, \
                COALESCE(SUM(CASE WHEN t.type = 'OUT'  THEN t.quantity END), 0) AS out_qty, \
                COALESCE(SUM(CASE WHEN t.type IN ('IN','OUT') THEN 1 ELSE 0 END), 0) AS tx_count, \
                COALESCE(SUM(CASE WHEN t.type = 'VOID' THEN 1 ELSE 0 END), 0) AS void_count \
         FROM {select_from} \
         WHERE {} \
         GROUP BY {group_col} \
         ORDER BY (in_qty + out_qty) DESC, group_key ASC",
        conds.join(" AND ")
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| map_db_err(&e, "统计查询"))?;
    let rows = stmt
        .query_map(params_from_iter(args), |r| {
            Ok(StatsRow {
                group_key: r.get(0)?,
                group_name: r.get(1)?,
                in_qty: r.get(2)?,
                out_qty: r.get(3)?,
                tx_count: r.get(4)?,
                void_count: r.get(5)?,
            })
        })
        .map_err(|e| map_db_err(&e, "统计查询"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "统计查询"))?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::partners::NewPartner;
    use crate::commands::transactions::{NewTransaction, TxType};
    use crate::db::test_db;

    fn wide_range() -> StatsQuery {
        StatsQuery {
            group_by: "item".into(),
            date_from: "2000-01-01T00:00:00.000Z".into(),
            date_to: "2100-01-01T00:00:00.000Z".into(),
            item_id: None,
            partner_id: None,
        }
    }

    #[test]
    fn group_by_item_partner_operator() {
        let db = test_db();
        let conn = db.lock();

        let a = crate::repo::items::create(
            &conn,
            &NewItem { name: "苹果汁".into(), code: None, spec: None, unit: "箱".into(), note: None },
        )
        .unwrap();
        let b = crate::repo::items::create(
            &conn,
            &NewItem { name: "香蕉".into(), code: None, spec: None, unit: "kg".into(), note: None },
        )
        .unwrap();
        let p = crate::repo::partners::create(
            &conn,
            &NewPartner { name: "老王批发部".into(), r#type: "supplier".into(), contact: None, note: None },
        )
        .unwrap();
        let op = crate::repo::operators::create(&conn, "张三").unwrap();

        let tx = |item: &str, t: TxType, qty: f64, partner: Option<&str>, when: &str| NewTransaction {
            tx_type: t,
            item_id: item.to_string(),
            quantity: qty,
            partner_id: partner.map(str::to_string),
            operator_id: op.id.clone(),
            remark: None,
            happened_at: Some(when.to_string()),
            copied_from: None,
        };

        crate::repo::transactions::create(&conn, &tx(&a.id, TxType::In, 10.0, Some(&p.id), "2026-02-01T02:00:00.000Z"), "dev").unwrap();
        crate::repo::transactions::create(&conn, &tx(&a.id, TxType::Out, 5.0, Some(&p.id), "2026-02-02T02:00:00.000Z"), "dev").unwrap();
        crate::repo::transactions::create(&conn, &tx(&b.id, TxType::In, 3.0, None, "2026-02-03T02:00:00.000Z"), "dev").unwrap();
        // 期间外的单据不计入
        crate::repo::transactions::create(&conn, &tx(&a.id, TxType::In, 100.0, Some(&p.id), "2001-06-01T02:00:00.000Z"), "dev").unwrap();

        // 作废 A 的入库 10 → void_count 计入 A 组
        let in_a: String = conn
            .query_row(
                "SELECT id FROM v_transaction_detail WHERE type = 'IN' AND item_id = ?1 AND happened_at >= '2026'",
                [a.id.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        crate::repo::transactions::void(&conn, &in_a, Some("录错了"), "dev").unwrap();

        // 按货物（限定 2026 期间）
        let mut q = wide_range();
        q.group_by = "item".into();
        q.date_from = "2026-01-01T00:00:00.000Z".into();
        q.date_to = "2026-12-31T23:59:59.999Z".into();
        let rows = query(&conn, &q).unwrap();
        assert_eq!(rows.len(), 2);
        // 排序：合计量大的在前（A: 10+5=15 > B: 3）
        assert_eq!(rows[0].group_key, a.id);
        assert_eq!(rows[0].in_qty, 10.0);
        assert_eq!(rows[0].out_qty, 5.0);
        assert_eq!(rows[0].tx_count, 2);
        assert_eq!(rows[0].void_count, 1);
        assert_eq!(rows[0].group_name, "苹果汁");
        assert_eq!(rows[1].group_key, b.id);
        assert_eq!(rows[1].tx_count, 1);
        assert_eq!(rows[1].void_count, 0);

        // 按往来单位：p 组 10/5，无单位单据归「未指定」组
        let mut q = wide_range();
        q.group_by = "partner".into();
        q.date_from = "2026-01-01T00:00:00.000Z".into();
        q.date_to = "2026-12-31T23:59:59.999Z".into();
        let rows = query(&conn, &q).unwrap();
        assert_eq!(rows.len(), 2);
        let p_row = rows.iter().find(|r| r.group_key == p.id).unwrap();
        assert_eq!(p_row.in_qty, 10.0);
        assert_eq!(p_row.out_qty, 5.0);
        assert_eq!(p_row.group_name, "老王批发部");
        let none_row = rows.iter().find(|r| r.group_key.is_empty()).unwrap();
        assert_eq!(none_row.group_name, "未指定");
        assert_eq!(none_row.in_qty, 3.0);

        // 按操作人（超集维度）
        let mut q = wide_range();
        q.group_by = "operator".into();
        let rows = query(&conn, &q).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].group_key, op.id);
        assert_eq!(rows[0].in_qty, 113.0, "含期间外的 100");
        assert_eq!(rows[0].tx_count, 4);
        assert_eq!(rows[0].void_count, 1);

        // 维度内筛选 item_id
        let mut q = wide_range();
        q.group_by = "partner".into();
        q.item_id = Some(b.id.clone());
        let rows = query(&conn, &q).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].group_key.is_empty());

        // 非法维度 → E_VALIDATION；非法时间 → E_VALIDATION；起止颠倒 → E_VALIDATION
        let mut q = wide_range();
        q.group_by = "device".into();
        assert_eq!(query(&conn, &q).unwrap_err().code, ErrorCode::Validation);
        let mut q = wide_range();
        q.date_from = "昨天".into();
        assert_eq!(query(&conn, &q).unwrap_err().code, ErrorCode::Validation);
        let mut q = wide_range();
        q.date_from = "2026-12-31T00:00:00.000Z".into();
        q.date_to = "2026-01-01T00:00:00.000Z".into();
        assert_eq!(query(&conn, &q).unwrap_err().code, ErrorCode::Validation);
    }
}
