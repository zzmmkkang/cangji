//! 往来单位域命令（契约 4.3）。共 4 条：list_partners / create_partner / update_partner /
//! set_partner_archived。与 items 同构（多 type，少 unit），无独立删除。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Db;
use crate::error::CommandError;

/// 往来单位类型："supplier" | "customer" | "both"（契约 4.3 PartnerType）。
pub type PartnerType = String;

/// 往来单位档案（契约 4.3 Partner，与 partners 表对应）；Clone 供数据包复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Partner {
    pub id: String,
    pub name: String,
    pub r#type: PartnerType,
    pub contact: Option<String>,
    pub note: Option<String>,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建往来单位入参。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPartner {
    pub name: String,
    pub r#type: PartnerType,
    pub contact: Option<String>,
    pub note: Option<String>,
}

/// 更新往来单位入参（部分更新）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePartner {
    pub name: Option<String>,
    pub r#type: Option<PartnerType>,
    pub contact: Option<String>,
    pub note: Option<String>,
}

/// 往来单位列表：keyword 模糊匹配名称；include_archived 控制是否含已归档。
#[tauri::command]
pub fn list_partners(
    db: State<'_, Db>,
    keyword: Option<String>,
    include_archived: Option<bool>,
) -> Result<Vec<Partner>, CommandError> {
    let conn = db.lock();
    crate::repo::partners::list(&conn, keyword.as_deref(), include_archived.unwrap_or(false))
}

/// 新建往来单位（主键 UUID v7）。
#[tauri::command]
pub fn create_partner(db: State<'_, Db>, input: NewPartner) -> Result<Partner, CommandError> {
    let conn = db.lock();
    crate::repo::partners::create(&conn, &input)
}

/// 更新往来单位（不存在 → E_NOT_FOUND）。
#[tauri::command]
pub fn update_partner(
    db: State<'_, Db>,
    id: String,
    input: UpdatePartner,
) -> Result<Partner, CommandError> {
    let conn = db.lock();
    crate::repo::partners::update(&conn, &id, &input)
}

/// 归档 / 取消归档。
#[tauri::command]
pub fn set_partner_archived(
    db: State<'_, Db>,
    id: String,
    archived: bool,
) -> Result<Partner, CommandError> {
    let conn = db.lock();
    crate::repo::partners::set_archived(&conn, &id, archived)
}
