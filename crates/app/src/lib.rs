pub mod commands;
pub mod db;
pub mod history_view;
pub mod rates;

use tauri::Manager;

use crate::db::Db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = tauri::async_runtime::block_on(Db::open(&dir.join("epicycle.sqlite3")))?;
            app.manage(db);
            rates::spawn_refresher(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::currencies,
            commands::add_subscription,
            commands::subscription,
            commands::edit_subscription,
            commands::delete_subscription,
            commands::export_backup,
            commands::preview_import,
            commands::import_backup,
            commands::subscription_history,
            commands::restore_revision,
            commands::deleted_subscriptions,
            commands::converted_total,
            commands::refresh_rates,
            commands::exchange_rates,
            commands::add_manual_rate,
            commands::delete_manual_rate,
            commands::set_reporting_currency,
            commands::favorite_currencies,
            commands::set_favorite_currencies,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Epicycle");
}
