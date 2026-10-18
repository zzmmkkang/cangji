//! 库存域命令（契约 4.5）。共 1 条：get_inventory。
//! 库存唯一事实来源 = v_inventory 视图实时推导；balance < 0 → negative 标红（D2）。

use serde::Serialize;
use tauri::State;

use crate::db::Db;
use crate::error::CommandError;

/// 库存行（契约 4.5 InventoryRow）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRow {
    pub item_id: String,
    pub item_name: String,
    pub spec: Option<String>,
    pub unit: String,
    pub total_in: f64,
    pub total_out: f64,
    pub total_voided: f64,
    pub balance: f64,
    /// balance < 0 → 前端标红（D2 允许负库存提交，不拦截）。
    pub negative: bool,
    /// 本地"今天"进出（UTC 边界由 Rust 按设备本地时区计算，契约 7.3）。
    pub today_in: f64,
    pub today_out: f64,
}

/// 库存总览：含已归档货物，前端自行折叠。
#[tauri::command]
pub fn get_inventory(db: State<'_, Db>) -> Result<Vec<InventoryRow>, CommandError> {
    let conn = db.lock();
    crate::repo::inventory::list(&conn)
}
