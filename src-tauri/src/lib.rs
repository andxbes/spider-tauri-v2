mod commands;
mod crawl;
mod settings;

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter};

const MENU_DUMP_SAVE: &str = "session-dump-save";
const MENU_DUMP_LOAD: &str = "session-dump-load";
const MENU_ABOUT: &str = "about-show";

fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    let save_dump = MenuItemBuilder::with_id(MENU_DUMP_SAVE, "Зберегти дамп сканування…")
        .accelerator("CmdOrCtrl+Shift+S")
        .build(app)?;
    let load_dump = MenuItemBuilder::with_id(MENU_DUMP_LOAD, "Завантажити дамп…")
        .accelerator("CmdOrCtrl+Shift+O")
        .build(app)?;
    let about = MenuItemBuilder::with_id(MENU_ABOUT, "Про Tauri Web Spider…").build(app)?;

    let file_menu = SubmenuBuilder::new(app, "Файл")
        .item(&save_dump)
        .item(&load_dump)
        .separator()
        .quit()
        .build()?;

    let help_menu = SubmenuBuilder::new(app, "Про програму").item(&about).build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&file_menu, &help_menu])
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        MENU_DUMP_SAVE => {
            let _ = app.emit("session-dump-request-save", ());
        }
        MENU_DUMP_LOAD => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                match commands::session_load(app.clone()).await {
                    Ok(payload) => {
                        if payload.get("canceled").and_then(|v| v.as_bool()) == Some(true) {
                            return;
                        }
                        let _ = app.emit("session-dump-loaded", payload);
                    }
                    Err(error) => {
                        let _ = app.emit(
                            "session-dump-loaded",
                            serde_json::json!({ "ok": false, "error": error }),
                        );
                    }
                }
            });
        }
        MENU_ABOUT => {
            let _ = app.emit("about-show", ());
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let handle = app.handle().clone();
            if let Err(error) = build_menu(&handle) {
                eprintln!("Не вдалося створити меню: {error}");
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_about,
            commands::settings_get,
            commands::settings_save,
            commands::open_external,
            commands::start_spider,
            commands::spider_pause,
            commands::spider_resume,
            commands::spider_stop,
            commands::session_save,
            commands::session_save_json,
            commands::session_load,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
