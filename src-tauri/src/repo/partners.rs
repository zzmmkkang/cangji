//! 往来单位仓储（契约 3.1 partners 表）。与 items 同构：多 type 枚举、少 unit。
//! 说明：partners.name 不建 UNIQUE——契约 3.4「重名建档允许」，是否提示由应用层处理。

use rusqlite::{params, params_from_iter, Connection, ToSql};
use uuid::Uuid;

use crate::commands::partners::{NewPartner, Partner, UpdatePartner};
use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};

use super::{map_db_err, norm_opt};

const COLS: &str = "id, name, type, contact, note, is_archived, created_at, updated_at";

/// type 枚举（契约 3.1 CHECK 约束）：supplier | customer | both。
pub(crate) const PARTNER_TYPES: [&str; 3] = ["supplier", "customer", "both"];

fn row_to_partner(row: &rusqlite::Row<'_>) -> rusqlite::Result<Partner> {
    Ok(Partner {
        id: row.get(0)?,
        name: row.get(1)?,
        r#type: row.get(2)?,
        contact: row.get(3)?,
        note: row.get(4)?,
        is_archived: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn validate_name(name: &str) -> Result<(), CommandError> {
    if name.trim().is_empty() {
        return Err(CommandError::new(ErrorCode::Validation, "单位名称不能为空"));
    }
    Ok(())
}

fn validate_type(partner_type: &str) -> Result<(), CommandError> {
    if !PARTNER_TYPES.contains(&partner_type) {
        return Err(
            CommandError::new(ErrorCode::Validation, "单位类型必须为 supplier / customer / both")
                .with_detail(serde_json::json!({ "field": "type", "value": partner_type })),
        );
    }
    Ok(())
}

/// 列表：keyword 模糊匹配名称；默认排除已归档；按 updated_at DESC（契约 7.7）。
pub fn list(
    conn: &Connection,
    keyword: Option<&str>,
    include_archived: bool,
) -> Result<Vec<Partner>, CommandError> {
    let kw_pattern = keyword
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| format!("%{k}%"));

    let mut sql = format!("SELECT {COLS} FROM partners WHERE 1=1");
    let mut args: Vec<&dyn ToSql> = Vec::new();
    if let Some(pattern) = &kw_pattern {
        sql.push_str(" AND name LIKE ?1");
        args.push(pattern);
    }
    if !include_archived {
        sql.push_str(" AND is_archived = 0");
    }
    sql.push_str(" ORDER BY updated_at DESC, id DESC");

    let mut stmt = conn.prepare(&sql).map_err(|e| map_db_err(&e, "查询往来单位列表"))?;
    let rows = stmt
        .query_map(params_from_iter(args), row_to_partner)
        .map_err(|e| map_db_err(&e, "查询往来单位列表"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "查询往来单位列表"))?;
    Ok(rows)
}

/// 按 id 取档案；不存在 → E_NOT_FOUND。
pub fn get(conn: &Connection, id: &str) -> Result<Partner, CommandError> {
    conn.query_row(&format!("SELECT {COLS} FROM partners WHERE id = ?1"), [id], row_to_partner)
        .map_err(|e| map_db_err(&e, "查询往来单位"))
}

/// 新建（UUID v7 主键；type 枚举校验）。
pub fn create(conn: &Connection, input: &NewPartner) -> Result<Partner, CommandError> {
    validate_name(&input.name)?;
    validate_type(&input.r#type)?;

    let id = Uuid::now_v7().to_string();
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO partners (id, name, type, contact, note, is_archived, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
        params![
            id,
            input.name.trim(),
            input.r#type,
            norm_opt(input.contact.clone()),
            norm_opt(input.note.clone()),
            now
        ],
    )
    .map_err(|e| map_db_err(&e, "新建往来单位"))?;
    get(conn, &id)
}

/// 部分更新（None = 不改；可空字段传空串 = 置 NULL），刷新 updated_at。
pub fn update(conn: &Connection, id: &str, input: &UpdatePartner) -> Result<Partner, CommandError> {
    let existing = get(conn, id)?;

    let name = input.name.as_deref().map(str::trim).unwrap_or(existing.name.as_str());
    validate_name(name)?;
    let partner_type = input.r#type.as_deref().unwrap_or(existing.r#type.as_str());
    validate_type(partner_type)?;
    let contact = match &input.contact {
        None => existing.contact.clone(),
        Some(v) => norm_opt(Some(v.clone())),
    };
    let note = match &input.note {
        None => existing.note.clone(),
        Some(v) => norm_opt(Some(v.clone())),
    };

    conn.execute(
        "UPDATE partners SET name = ?1, type = ?2, contact = ?3, note = ?4, updated_at = ?5 \
         WHERE id = ?6",
        params![name, partner_type, contact, note, now_rfc3339(), id],
    )
    .map_err(|e| map_db_err(&e, "更新往来单位"))?;
    get(conn, id)
}

/// 归档 / 取消归档；不存在 → E_NOT_FOUND。
pub fn set_archived(conn: &Connection, id: &str, archived: bool) -> Result<Partner, CommandError> {
    let affected = conn
        .execute(
            "UPDATE partners SET is_archived = ?1, updated_at = ?2 WHERE id = ?3",
            params![archived as i64, now_rfc3339(), id],
        )
        .map_err(|e| map_db_err(&e, "归档往来单位"))?;
    if affected == 0 {
        return Err(
            CommandError::new(ErrorCode::NotFound, "往来单位不存在")
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

    fn new_partner(name: &str, partner_type: &str) -> NewPartner {
        NewPartner {
            name: name.to_string(),
            r#type: partner_type.to_string(),
            contact: None,
            note: None,
        }
    }

    #[test]
    fn create_list_update_archive_chain() {
        let db = test_db();
        let conn = db.lock();

        let p = create(&conn, &new_partner("老王批发部", "supplier")).unwrap();
        assert_eq!(p.r#type, "supplier");
        assert!(p.id < Uuid::now_v7().to_string(), "v7 主键时间有序");

        let p2 = create(&conn, &new_partner("张三超市", "customer")).unwrap();
        assert_eq!(list(&conn, None, false).unwrap().len(), 2);

        let hit = list(&conn, Some("老王"), false).unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].id, p.id);

        // update 改名 + type + 可空字段
        let updated = update(
            &conn,
            &p2.id,
            &UpdatePartner {
                name: Some("李四便利店".into()),
                r#type: Some("both".into()),
                contact: Some("13800000000".into()),
                note: Some("常客".into()),
            },
        )
        .unwrap();
        assert_eq!(updated.name, "李四便利店");
        assert_eq!(updated.r#type, "both");
        assert_eq!(updated.contact.as_deref(), Some("13800000000"));

        // 归档软删
        assert!(set_archived(&conn, &p2.id, true).unwrap().is_archived);
        assert_eq!(list(&conn, None, false).unwrap().len(), 1);
        assert_eq!(list(&conn, None, true).unwrap().len(), 2);

        let err = set_archived(&conn, "nope", true).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn type_enum_is_validated() {
        let db = test_db();
        let conn = db.lock();
        let err = create(&conn, &new_partner("某单位", "friend")).unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
        assert_eq!(err.detail.as_ref().unwrap()["field"], "type");
    }

    #[test]
    fn empty_name_is_validation_error() {
        let db = test_db();
        let conn = db.lock();
        let err = create(&conn, &new_partner("", "supplier")).unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
    }

    #[test]
    fn duplicate_partner_names_are_allowed_by_contract() {
        // 契约 3.4：partners.name 不建 UNIQUE，重名建档允许
        let db = test_db();
        let conn = db.lock();
        create(&conn, &new_partner("同名单位", "supplier")).unwrap();
        create(&conn, &new_partner("同名单位", "customer")).unwrap();
        assert_eq!(list(&conn, None, true).unwrap().len(), 2);
    }
}
