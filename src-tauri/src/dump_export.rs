//! Stream session dumps to disk in small chunks.
//! Avoids shipping a 100MB+ JSON string through WebKit → IPC (OOM / crash).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};

pub fn default_dump_name(start_url: &str) -> String {
    let host = url::Url::parse(start_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "scan".into())
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let stamp = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
    format!("spider_{host}_{stamp}.spider.json")
}

/// Open a save dialog and return the chosen path (no dump body involved).
#[tauri::command]
pub async fn session_save_pick(app: AppHandle, start_url: Option<String>) -> Result<Value, String> {
    let file_path = app
        .dialog()
        .file()
        .set_title("Зберегти дамп сканування")
        .set_file_name(&default_dump_name(start_url.as_deref().unwrap_or("")))
        .add_filter("Дамп сканування Spider", &["spider.json", "json"])
        .blocking_save_file();

    let Some(FilePath::Path(path)) = file_path else {
        return Ok(serde_json::json!({ "ok": false, "canceled": true }));
    };
    Ok(serde_json::json!({
        "ok": true,
        "filePath": path.to_string_lossy(),
    }))
}

/// Write or append a UTF-8 chunk to the dump file.
/// First chunk: `truncate = true`. Subsequent: `truncate = false` (append).
#[tauri::command]
pub async fn session_dump_write_chunk(
    path: String,
    data: String,
    truncate: bool,
) -> Result<Value, String> {
    if path.is_empty() {
        return Err("Порожній шлях до файлу дампу.".into());
    }
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let path_for_io = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(truncate)
            .append(!truncate)
            .open(&path_for_io)
            .map_err(|e| e.to_string())?;
        file.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(serde_json::json!({ "ok": true }))
}
