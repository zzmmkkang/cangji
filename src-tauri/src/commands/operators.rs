//! 操作人域命令（契约 4.3）。共 3 条：list_operators / create_operator / update_operator。
//! 无独立管理页：新增入口在「记一笔」操作人选择器，改名入口在数据共账页。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Db;
use crate::error::CommandError;

/// 操作人（契约 4.3 Operator，与 operators 表对应）；
/// Deserialize/Clone 供数据包导入（pack/model.rs 反序列化）复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operator {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 操作人列表。
#[tauri::command]
pub fn list_operators(db: State<'_, Db>) -> Result<Vec<Operator>, CommandError> {
    let conn = db.lock();
    crate::repo::operators::list(&conn)
}

/// 新建操作人；name 全局唯一，重名 → E_DUPLICATE（契约 4.3）。
#[tauri::command]
pub fn create_operator(db: State<'_, Db>, name: String) -> Result<Operator, CommandError> {
    let conn = db.lock();
    crate::repo::operators::create(&conn, &name)
}

/// 操作人改名（历史单据经外键自动跟随，统计口径一致）。
#[tauri::command]
pub fn update_operator(
    db: State<'_, Db>,
    id: String,
    name: String,
) -> Result<Operator, CommandError> {
    let conn = db.lock();
    crate::repo::operators::update(&conn, &id, &name)
}
