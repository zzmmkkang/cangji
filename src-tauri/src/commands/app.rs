//! 应用信息 / 设置域命令（契约 4.1）。共 3 条：get_app_info / get_settings / set_setting。

use serde::Serialize;
use tauri::State;

use crate::db::Db;
use crate::error::CommandError;

/// 应用信息（契约 4.1 AppInfo）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_version: String,
    pub schema_version: u32,
    pub device_id: String,
    pub device_name: String,
    pub db_path: String,
    pub last_sync_at: Option<String>,
}

/// 单条设置（契约 4.1 Setting）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

/// 读取应用信息（版本、schema 版本、设备身份、DB 路径、距上次同步）。
#[tauri::command]
pub fn get_app_info(app: tauri::AppHandle, db: State<'_, Db>) -> Result<AppInfo, CommandError> {
    let conn = db.lock();
    let schema_version = crate::db::migrations::current_version(&conn)?;
    let device_id = crate::device::device_id(&conn)?;
    let device_name = crate::device::device_name(&conn)?;
    let last_sync_at = crate::repo::merge::last_sync_at(&conn)?;
    Ok(AppInfo {
        app_version: app.package_info().version.to_string(),
        schema_version,
        device_id,
        device_name,
        db_path: db.path().to_string(),
        last_sync_at,
    })
}

/// 读取全部设置键值对。
#[tauri::command]
pub fn get_settings(db: State<'_, Db>) -> Result<Vec<Setting>, CommandError> {
    let conn = db.lock();
    crate::repo::settings::get_all(&conn)
}

/// 写入一条设置（UPSERT）。
#[tauri::command]
pub fn set_setting(db: State<'_, Db>, key: String, value: String) -> Result<(), CommandError> {
    let conn = db.lock();
    crate::repo::settings::set(&conn, &key, &value)
}
