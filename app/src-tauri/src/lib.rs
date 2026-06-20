mod commands;
mod db;
mod hash;
mod importer;
mod meta;
mod model;
mod scanner;
mod slug;
pub mod taxonomy;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let home = dirs::home_dir().expect("home dir");
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("app data dir")
                .join("skill-library");
            let library_root = data_dir.join("library");
            std::fs::create_dir_all(&library_root).expect("create library dir");
            let conn = db::open(&data_dir.join("catalog.db")).expect("open db");

            // The bundled inventory tarball, if present in the repo.
            let tarball_path = home.join("Repo/skills/skills-inventory/skills-deduped.tar.gz");
            let tarball_path = if tarball_path.exists() {
                Some(tarball_path)
            } else {
                None
            };

            app.manage(commands::AppState {
                db: std::sync::Mutex::new(conn),
                library_root,
                home,
                tarball_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::list_locations,
            commands::run_import,
            commands::get_item_content,
            commands::list_scan_dirs,
            commands::add_scan_dir,
            commands::remove_scan_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
