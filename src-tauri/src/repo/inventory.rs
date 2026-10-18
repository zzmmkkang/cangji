//! 库存查询（契约 3.1 v_inventory 视图：balance = ΣIN − ΣOUT + ΣVOID(原单OUT) − ΣVOID(原单IN)，唯一事实来源）。
//! 含已归档货物（前端自行折叠）；today_in/today_out 按设备本地时区的"今天"边界统计（契约 7.3）。

use chrono::{DateTime, Duration, Local, LocalResult, Utc};
use rusqlite::{params, Connection};

use crate::commands::inventory::InventoryRow;
use crate::error::CommandError;

use super::map_db_err;

/// 设备本地时区的"今天"起止（转 UTC RFC3339 毫秒；结束为次日零点，左闭右开）。
fn local_today_bounds() -> (String, String) {
    let midnight = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let start: DateTime<Utc> = match midnight.and_local_timezone(Local) {
        LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
        // DST 跳变导致本地零点不存在的极端场景：按 UTC 零点兜底
        LocalResult::None => midnight.and_utc(),
    };
    let end = start + Duration::days(1);
    let fmt = |t: DateTime<Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    (fmt(start), fmt(end))
}

/// 库存总览：v_inventory 全量（含已归档）+ 今日进出；negative = balance < 0（D2 标红判定）。
pub fn list(conn: &Connection) -> Result<Vec<InventoryRow>, CommandError> {
    let (today_start, today_end) = local_today_bounds();
    let sql = "SELECT v.item_id, v.item_name, v.spec, v.unit, v.total_in, v.total_out, \
                      v.total_voided, v.balance, COALESCE(d.today_in, 0), COALESCE(d.today_out, 0) \
               FROM v_inventory v \
               LEFT JOIN ( \
                   SELECT item_id, \
                          COALESCE(SUM(CASE WHEN type = 'IN'  THEN quantity END), 0) AS today_in, \
                          COALESCE(SUM(CASE WHEN type = 'OUT' THEN quantity END), 0) AS today_out \
                   FROM transactions \
                   WHERE happened_at >= ?1 AND happened_at < ?2 \
                   GROUP BY item_id \
               ) d ON d.item_id = v.item_id \
               ORDER BY v.is_archived ASC, v.item_name ASC";

    let mut stmt = conn.prepare(sql).map_err(|e| map_db_err(&e, "查询库存"))?;
    let rows = stmt
        .query_map(params![today_start, today_end], |r| {
            let balance: f64 = r.get(7)?;
            Ok(InventoryRow {
                item_id: r.get(0)?,
                item_name: r.get(1)?,
                spec: r.get(2)?,
                unit: r.get(3)?,
                total_in: r.get(4)?,
                total_out: r.get(5)?,
                total_voided: r.get(6)?,
                balance,
                negative: balance < 0.0,
                today_in: r.get(8)?,
                today_out: r.get(9)?,
            })
        })
        .map_err(|e| map_db_err(&e, "查询库存"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "查询库存"))?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::items::NewItem;
    use crate::commands::transactions::{NewTransaction, TxType};
    use crate::db::test_db;

    #[test]
    fn inventory_covers_archived_items_and_today_bounds() {
        let db = test_db();
        let conn = db.lock();

        let item = crate::repo::items::create(
            &conn,
            &NewItem { name: "面粉".into(), code: None, spec: None, unit: "袋".into(), note: None },
        )
        .unwrap();
        let operator = crate::repo::operators::create(&conn, "赵六").unwrap();

        let tx = |tx_type: TxType, qty: f64, happened_at: &str| NewTransaction {
            tx_type,
            item_id: item.id.clone(),
            quantity: qty,
            partner_id: None,
            operator_id: operator.id.clone(),
            remark: None,
            happened_at: Some(happened_at.to_string()),
            copied_from: None,
        };

        // 今天的入库 + 昨天的出库
        let now_utc = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let yesterday = (Utc::now() - Duration::days(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        crate::repo::transactions::create(&conn, &tx(TxType::In, 10.0, &now_utc), "dev").unwrap();
        crate::repo::transactions::create(&conn, &tx(TxType::Out, 4.0, &yesterday), "dev").unwrap();

        // 归档后仍在库存总览中出现（含已归档货物，前端自行折叠）
        crate::repo::items::set_archived(&conn, &item.id, true).unwrap();

        let rows = list(&conn).unwrap();
        let row = rows.iter().find(|r| r.item_id == item.id).unwrap();
        assert_eq!(row.total_in, 10.0);
        assert_eq!(row.total_out, 4.0);
        assert_eq!(row.balance, 6.0);
        assert!(!row.negative);
        assert_eq!(row.today_in, 10.0, "今日边界按本地时区计算");
        assert_eq!(row.today_out, 0.0, "昨天的出库不计入今日");
    }

    #[test]
    fn empty_inventory_returns_no_rows() {
        let db = test_db();
        let conn = db.lock();
        assert!(list(&conn).unwrap().is_empty());
    }
}
