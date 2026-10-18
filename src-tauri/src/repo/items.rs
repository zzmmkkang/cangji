//! 货物档案仓储（契约 3.1 items 表）。
//! 说明：items.name 不建 UNIQUE——契约 3.4「重名建档允许」，同名是否提示由应用层处理，
//! 本层只做必填校验与软删（is_archived）。

use rusqlite::{params, params_from_iter, Connection, ToSql};
use uuid::Uuid;

use crate::commands::items::{Item, NewItem, UpdateItem};
use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};

use super::{map_db_err, norm_opt};

const COLS: &str = "id, name, code, spec, unit, note, is_archived, created_at, updated_at";

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        name: row.get(1)?,
        code: row.get(2)?,
        spec: row.get(3)?,
        unit: row.get(4)?,
        note: row.get(5)?,
        is_archived: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn require_non_empty(value: &str, field: &str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::new(ErrorCode::Validation, format!("{field}不能为空")));
    }
    Ok(())
}

/// 列表：keyword 模糊匹配名称/编码（A1 命令注释口径）；默认排除已归档；按 updated_at DESC（契约 7.7）。
pub fn list(
    conn: &Connection,
    keyword: Option<&str>,
    include_archived: bool,
) -> Result<Vec<Item>, CommandError> {
    let kw_pattern = keyword
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| format!("%{k}%"));

    let mut sql = format!("SELECT {COLS} FROM items WHERE 1=1");
    let mut args: Vec<&dyn ToSql> = Vec::new();
    if let Some(pattern) = &kw_pattern {
        sql.push_str(" AND (name LIKE ?1 OR code LIKE ?1)");
        args.push(pattern);
    }
    if !include_archived {
        sql.push_str(" AND is_archived = 0");
    }
    sql.push_str(" ORDER BY updated_at DESC, id DESC");

    let mut stmt = conn.prepare(&sql).map_err(|e| map_db_err(&e, "查询货物列表"))?;
    let rows = stmt
        .query_map(params_from_iter(args), row_to_item)
        .map_err(|e| map_db_err(&e, "查询货物列表"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "查询货物列表"))?;
    Ok(rows)
}

/// 按 id 取档案；不存在 → E_NOT_FOUND。
pub fn get(conn: &Connection, id: &str) -> Result<Item, CommandError> {
    conn.query_row(&format!("SELECT {COLS} FROM items WHERE id = ?1"), [id], row_to_item)
        .map_err(|e| map_db_err(&e, "查询货物"))
}

/// 新建（UUID v7 主键 + created_at/updated_at，契约 7.1）。
pub fn create(conn: &Connection, input: &NewItem) -> Result<Item, CommandError> {
    require_non_empty(&input.name, "货物名称")?;
    require_non_empty(&input.unit, "计量单位")?;

    let id = Uuid::now_v7().to_string();
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO items (id, name, code, spec, unit, note, is_archived, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
        params![
            id,
            input.name.trim(),
            norm_opt(input.code.clone()),
            norm_opt(input.spec.clone()),
            input.unit.trim(),
            norm_opt(input.note.clone()),
            now
        ],
    )
    .map_err(|e| map_db_err(&e, "新建货物"))?;
    get(conn, &id)
}

/// 部分更新（None = 不改；可空字段传空串 = 置 NULL），刷新 updated_at（合并 LWW 字段）。
pub fn update(conn: &Connection, id: &str, input: &UpdateItem) -> Result<Item, CommandError> {
    let existing = get(conn, id)?;

    let name = input.name.as_deref().map(str::trim).unwrap_or(existing.name.as_str());
    require_non_empty(name, "货物名称")?;
    let unit = input.unit.as_deref().map(str::trim).unwrap_or(existing.unit.as_str());
    require_non_empty(unit, "计量单位")?;
    let code = match &input.code {
        None => existing.code.clone(),
        Some(v) => norm_opt(Some(v.clone())),
    };
    let spec = match &input.spec {
        None => existing.spec.clone(),
        Some(v) => norm_opt(Some(v.clone())),
    };
    let note = match &input.note {
        None => existing.note.clone(),
        Some(v) => norm_opt(Some(v.clone())),
    };

    conn.execute(
        "UPDATE items SET name = ?1, code = ?2, spec = ?3, unit = ?4, note = ?5, updated_at = ?6 \
         WHERE id = ?7",
        params![name, code, spec, unit, note, now_rfc3339(), id],
    )
    .map_err(|e| map_db_err(&e, "更新货物"))?;
    get(conn, id)
}

/// 归档 / 取消归档（软删除，契约 3.4）；不存在 → E_NOT_FOUND。
pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> Result<Item, CommandError> {
    let affected = conn
        .execute(
            "UPDATE items SET is_archived = ?1, updated_at = ?2 WHERE id = ?3",
            params![archived as i64, now_rfc3339(), id],
        )
        .map_err(|e| map_db_err(&e, "归档货物"))?;
    if affected == 0 {
        return Err(
            CommandError::new(ErrorCode::NotFound, "货物不存在")
                .with_detail(serde_json::json!({ "field": "id", "value": id })),
        );
    }
    get(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;
    use crate::error::ErrorCode;

    fn new_item(name: &str, unit: &str) -> NewItem {
        NewItem {
            name: name.to_string(),
            code: None,
            spec: None,
            unit: unit.to_string(),
            note: None,
        }
    }

    #[test]
    fn create_then_list_full_chain() {
        let db = test_db();
        let conn = db.lock();

        let a = create(&conn, &new_item("苹果汁", "箱")).unwrap();
        let b = create(&conn, &new_item("香蕉", "kg")).unwrap();

        // UUID v7 时间有序：后建的 id 字典序更大
        assert!(a.id < b.id, "{} < {}", a.id, b.id);
        // created_at/updated_at 为 UTC RFC3339 毫秒
        for ts in [&a.created_at, &a.updated_at] {
            assert_eq!(ts.len(), 24, "{ts}");
            assert!(ts.ends_with('Z'));
            chrono::DateTime::parse_from_rfc3339(ts).unwrap();
        }

        // 全量列表（按 updated_at DESC）
        let all = list(&conn, None, false).unwrap();
        assert_eq!(all.len(), 2);

        // keyword 模糊过滤（名称/编码）
        let hit = list(&conn, Some("苹果"), false).unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].id, a.id);
        assert!(list(&conn, Some("不存在的货物"), false).unwrap().is_empty());

        // 编码匹配
        conn.execute("UPDATE items SET code = ?1 WHERE id = ?2", params!["SKU-001", b.id])
            .unwrap();
        let by_code = list(&conn, Some("sku-001"), false).unwrap();
        assert_eq!(by_code.len(), 1);
        assert_eq!(by_code[0].id, b.id);
    }

    #[test]
    fn archive_filters_list_and_is_persistent() {
        let db = test_db();
        let conn = db.lock();
        let a = create(&conn, &new_item("可乐", "箱")).unwrap();

        let archived = set_archived(&conn, &a.id, true).unwrap();
        assert!(archived.is_archived);
        assert!(list(&conn, None, false).unwrap().is_empty(), "默认过滤已归档");
        assert_eq!(list(&conn, None, true).unwrap().len(), 1);

        let restored = set_archived(&conn, &a.id, false).unwrap();
        assert!(!restored.is_archived);
        assert_eq!(list(&conn, None, false).unwrap().len(), 1);

        let err = set_archived(&conn, "no-such-id", true).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn update_merges_partial_fields() {
        let db = test_db();
        let conn = db.lock();
        let item = create(&conn, &new_item("大米", "袋")).unwrap();

        let updated = update(
            &conn,
            &item.id,
            &UpdateItem {
                name: Some("东北大米".into()),
                code: Some("DB-01".into()),
                spec: None,
                unit: None,
                note: None,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "东北大米");
        assert_eq!(updated.code.as_deref(), Some("DB-01"));
        assert_eq!(updated.unit, "袋", "None 字段保持不变");

        // 可空字段传空串 = 清空（置 NULL）
        let cleared = update(
            &conn,
            &item.id,
            &UpdateItem { name: None, code: Some("".into()), spec: None, unit: None, note: None },
        )
        .unwrap();
        assert_eq!(cleared.code, None);

        let err = update(
            &conn,
            "no-such-id",
            &UpdateItem { name: Some("x".into()), code: None, spec: None, unit: None, note: None },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn empty_name_or_unit_is_validation_error() {
        let db = test_db();
        let conn = db.lock();
        let err = create(&conn, &new_item("   ", "箱")).unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
        let err = create(&conn, &new_item("水", "  ")).unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
    }

    #[test]
    fn duplicate_item_names_are_allowed_by_contract() {
        // 契约 3.4：items.name 不建 UNIQUE，重名建档允许（不返回 E_DUPLICATE）
        let db = test_db();
        let conn = db.lock();
        create(&conn, &new_item("可乐", "箱")).unwrap();
        create(&conn, &new_item("可乐", "瓶")).unwrap();
        assert_eq!(list(&conn, None, true).unwrap().len(), 2);
    }
}
