//! 仓记 CangJi 应用入口库（契约 2 章）：注册插件 + 25 条命令（24 条契约 + 1 条偏离）。
//! 命令注册核对单见契约 4.8，与 commands/ 各域一一对应。

mod commands;
mod db;
mod device;
mod error;
mod pack;
mod repo;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            // 日志：stdout（开发）+ 文件（app_log_dir）+ WebView console（契约 7.6）
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .setup(|app| {
            use tauri::Manager;

            // 桌面端防双开（契约 1.2）：Windows 避免两实例写同一 SQLite 库
            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_single_instance::init(
                |_app, _args, _cwd| {
                    // 已有实例运行时：A3 可在此聚焦既有主窗口
                },
            ))?;

            // 数据库：app_local_data_dir()/cangji.db（契约 3 章），打开即迁移（契约 3.6）。
            // 初始化失败 → panic 启动中止，不静默带病运行。
            let data_dir = app
                .path()
                .app_local_data_dir()
                .expect("无法解析应用数据目录（app_local_data_dir）");
            std::fs::create_dir_all(&data_dir).expect("创建应用数据目录失败");
            let db = db::Db::open(data_dir.join("cangji.db")).expect("数据库初始化失败，启动中止");
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 应用 / 设置（3）
            commands::app::get_app_info,
            commands::app::get_settings,
            commands::app::set_setting,
            // 货物（4）
            commands::items::list_items,
            commands::items::create_item,
            commands::items::update_item,
            commands::items::set_item_archived,
            // 往来单位（4）
            commands::partners::list_partners,
            commands::partners::create_partner,
            commands::partners::update_partner,
            commands::partners::set_partner_archived,
            // 操作人（3）
            commands::operators::list_operators,
            commands::operators::create_operator,
            commands::operators::update_operator,
            // 流水（3）
            commands::transactions::create_transaction,
            commands::transactions::void_transaction,
            commands::transactions::list_transactions,
            // 库存（1）
            commands::inventory::get_inventory,
            // 统计（2）
            commands::stats::get_stats,
            commands::stats::export_stats_excel,
            // 数据包 / 备份（4 + 1 新增）
            commands::pack::export_data_pack,
            commands::pack::inspect_data_pack,
            commands::pack::import_data_pack,
            commands::backup::backup_database,
            // 合并日志（【契约偏离】4.8 核对单 24 条之外的第 25 条，供数据共账页审计展示）
            commands::pack::list_merge_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running cangji");
}
