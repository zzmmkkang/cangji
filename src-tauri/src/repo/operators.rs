//! 操作人仓储（契约 3.1 operators 表，name 全局唯一 → 重名 E_DUPLICATE）。

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::commands::operators::Operator;
use crate::db::now_rfc3339;
use crate::error::{CommandError, ErrorCode};

use super::map_db_err;

const COLS: &str = "id, name, created_at, updated_at";

fn row_to_operator(row: &rusqlite::Row<'_>) -> rusqlite::Result<Operator> {
    Ok(Operator {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn validate_name(name: &str) -> Result<(), CommandError> {
    if name.trim().is_empty() {
        return Err(CommandError::new(ErrorCode::Validation, "操作人姓名不能为空"));
    }
    Ok(())
}

fn name_taken(conn: &Connection, name: &str, exclude_id: Option<&str>) -> Result<bool, CommandError> {
    let sql = match exclude_id {
        Some(_) => "SELECT EXISTS(SELECT 1 FROM operators WHERE name = ?1 AND id <> ?2)",
        None => "SELECT EXISTS(SELECT 1 FROM operators WHERE name = ?1)",
    };
    let exists: bool = match exclude_id {
        Some(exclude) => conn
            .query_row(sql, params![name, exclude], |r| r.get(0))
            .map_err(|e| map_db_err(&e, "查重操作人"))?,
        None => conn
            .query_row(sql, params![name], |r| r.get(0))
            .map_err(|e| map_db_err(&e, "查重操作人"))?,
    };
    Ok(exists)
}

/// 操作人列表（按 updated_at DESC）。
pub fn list(conn: &Connection) -> Result<Vec<Operator>, CommandError> {
    let mut stmt = conn
        .prepare(&format!("SELECT {COLS} FROM operators ORDER BY updated_at DESC, id DESC"))
        .map_err(|e| map_db_err(&e, "查询操作人列表"))?;
    let rows = stmt
        .query_map([], row_to_operator)
        .map_err(|e| map_db_err(&e, "查询操作人列表"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_db_err(&e, "查询操作人列表"))?;
    Ok(rows)
}

/// 按 id 取操作人；不存在 → E_NOT_FOUND。
pub fn get(conn: &Connection, id: &str) -> Result<Operator, CommandError> {
    conn.query_row(&format!("SELECT {COLS} FROM operators WHERE id = ?1"), [id], row_to_operator)
        .map_err(|e| map_db_err(&e, "查询操作人"))
}

/// 新建：name 唯一，重名 → E_DUPLICATE（契约 4.3）。
pub fn create(conn: &Connection, name: &str) -> Result<Operator, CommandError> {
    let name = name.trim();
    validate_name(name)?;
    if name_taken(conn, name, None)? {
        return Err(duplicate_name(name));
    }

    let id = Uuid::now_v7().to_string();
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO operators (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![id, name, now],
    )
    .map_err(|e| map_db_err(&e, "新建操作人"))?;
    get(conn, &id)
}

/// 改名（历史单据经外键自动跟随）；重名 → E_DUPLICATE，不存在 → E_NOT_FOUND。
pub fn update(conn: &Connection, id: &str, name: &str) -> Result<Operator, CommandError> {
    get(conn, id)?;
    let name = name.trim();
    validate_name(name)?;
    if name_taken(conn, name, Some(id))? {
        return Err(duplicate_name(name));
    }

    conn.execute(
        "UPDATE operators SET name = ?1, updated_at = ?2 WHERE id = ?3",
        params![name, now_rfc3339(), id],
    )
    .map_err(|e| map_db_err(&e, "更新操作人"))?;
    get(conn, id)
}

fn duplicate_name(name: &str) -> CommandError {
    CommandError::new(ErrorCode::Duplicate, format!("操作人「{name}」已存在（重名）"))
        .with_detail(serde_json::json!({ "field": "name", "value": name }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn create_list_and_duplicate_name() {
        let db = test_db();
        let conn = db.lock();

        let op = create(&conn, "张三").unwrap();
        assert_eq!(op.name, "张三");
        assert!(op.id < Uuid::now_v7().to_string(), "v7 主键时间有序");
        assert_eq!(list(&conn).unwrap().len(), 1);

        // 重名 → E_DUPLICATE（契约 4.3 / 7.5）
        let err = create(&conn, "张三").unwrap_err();
        assert_eq!(err.code, ErrorCode::Duplicate);
        assert_eq!(err.detail.as_ref().unwrap()["field"], "name");

        // 同名不同空白容忍：trim 后仍视为重名
        let err = create(&conn, "  张三 ").unwrap_err();
        assert_eq!(err.code, ErrorCode::Duplicate);

        let other = create(&conn, "李四").unwrap();
        assert_eq!(list(&conn).unwrap().len(), 2);

        // 改名重名 → E_DUPLICATE；改到唯一名 → 成功
        let err = update(&conn, &other.id, "张三").unwrap_err();
        assert_eq!(err.code, ErrorCode::Duplicate);
        let renamed = update(&conn, &other.id, "李四四").unwrap();
        assert_eq!(renamed.name, "李四四");

        // 不存在 → E_NOT_FOUND
        let err = update(&conn, "no-such-id", "王五").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn empty_name_is_validation_error() {
        let db = test_db();
        let conn = db.lock();
        let err = create(&conn, "   ").unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation);
    }

    #[test]
    fn db_unique_constraint_maps_to_duplicate() {
        // 绕过应用层查重直接落库 → UNIQUE 约束冲突 → E_DUPLICATE（错误映射路径）
        let db = test_db();
        let conn = db.lock();
        let op = create(&conn, "王五").unwrap();
        let err = conn
            .execute(
                "INSERT INTO operators (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                params![Uuid::now_v7().to_string(), op.name, now_rfc3339()],
            )
            .map_err(|e| map_db_err(&e, "新建操作人"))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Duplicate);
        assert!(err.detail.as_ref().unwrap()["fields"]
            .as_str()
            .unwrap()
            .contains("operators.name"));
    }
}
