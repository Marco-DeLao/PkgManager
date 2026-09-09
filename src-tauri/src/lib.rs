pub mod adapters;
pub mod cleanup;
pub mod commands;
pub mod distro;
pub mod install;
pub mod updates;
pub mod orphans;
pub mod matcher;
pub mod models;
pub mod path_picker;
pub mod scanner;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Debug)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::ping_command,
      commands::start_scan,
      commands::cleanup_preview,
      commands::cleanup_package,
      commands::pick_scan_path,
      commands::search_packages,
      commands::install_preview,
      commands::install_package,
      commands::cancel_install,
      commands::check_updates,
      commands::update_all,
      commands::check_orphans,
      commands::remove_orphans,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
