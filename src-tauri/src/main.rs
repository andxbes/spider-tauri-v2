// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux native menubar inherits GTK theme colors. On Manjaro/KDE (and mixed
    // dark/light setups) that often yields white label text on a light bar.
    // Force a coherent light GTK theme to match the app chrome.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("GTK_THEME").is_none() {
            std::env::set_var("GTK_THEME", "Adwaita");
        }
        if std::env::var_os("GTK_APPLICATION_PREFER_DARK_THEME").is_none() {
            std::env::set_var("GTK_APPLICATION_PREFER_DARK_THEME", "0");
        }
    }

    spider_tauri_lib::run();
}
