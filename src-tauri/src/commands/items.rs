//! 货物域命令（契约 4.2）。共 4 条：list_items / create_item / update_item / set_item_archived。
//! 档案可改（LWW 合并），"删除"= 置 is_archived（软删，契约 3.4）。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Db;
use crate::error::CommandError;

/// 货物档案（契约 4.2 Item，与 items 表对应）；Clone 供数据包（pack/model.rs）复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub spec: Option<String>,
    pub unit: String,
    pub note: Option<String>,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建货物入参。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewItem {
    pub name: String,
    pub code: Option<String>,
    pub spec: Option<String>,
    pub unit: String,
    pub note: Option<String>,
}

/// 更新货物入参（部分更新，None = 不改）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    pub name: Option<String>,
    pub code: Option<String>,
    pub spec: Option<String>,
    pub unit: Option<String>,
    pub note: Option<String>,
}

/// 货物列表：keyword 模糊匹配名称/编码；include_archived 控制是否含已归档。
/// 默认按 updated_at DESC（契约 7.7）。
#[tauri::command]
pub fn list_items(
    db: State<'_, Db>,
    keyword: Option<String>,
    include_archived: Option<bool>,
) -> Result<Vec<Item>, CommandError> {
    let conn = db.lock();
    crate::repo::items::list(&conn, keyword.as_deref(), include_archived.unwrap_or(false))
}

/// 新建货物档案（主键 UUID v7）。
#[tauri::command]
pub fn create_item(db: State<'_, Db>, input: NewItem) -> Result<Item, CommandError> {
    let conn = db.lock();
    crate::repo::items::create(&conn, &input)
}

/// 更新货物档案（不存在 → E_NOT_FOUND）。
#[tauri::command]
pub fn update_item(
    db: State<'_, Db>,
    id: String,
    input: UpdateItem,
) -> Result<Item, CommandError> {
    let conn = db.lock();
    crate::repo::items::update(&conn, &id, &input)
}

/// 归档 / 取消归档（软删除语义）。
#[tauri::command]
pub fn set_item_archived(
    db: State<'_, Db>,
    id: String,
    archived: bool,
) -> Result<Item, CommandError> {
    let conn = db.lock();
    crate::repo::items::set_archived(&conn, &id, archived)
}
