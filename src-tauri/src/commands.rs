use crate::crawl::{self, SpiderOptions};
use crate::settings::{self, AppSettings};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_opener::OpenerExt;

const DUMP_VERSION: u64 = 1;

#[derive(Serialize)]
pub struct AboutInfo {
    pub name: String,
    pub version: String,
    pub author: String,
    pub email: String,
}

#[tauri::command]
pub fn get_about() -> AboutInfo {
    AboutInfo {
        name: "Tauri Web Spider".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        author: "andxbes".into(),
        email: "andxbes@gmail.com".into(),
    }
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Result<Value, String> {
    let (settings, file_path) = settings::load_settings(&app)?;
    Ok(serde_json::json!({ "settings": settings, "filePath": file_path }))
}

#[tauri::command]
pub fn settings_save(app: AppHandle, settings: Value) -> Result<Value, String> {
    let (saved, file_path) = settings::save_settings(&app, settings)?;
    Ok(serde_json::json!({ "settings": saved, "filePath": file_path }))
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<Value, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Ok(serde_json::json!({ "ok": false, "error": "Invalid URL" }));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn start_spider(app: AppHandle, start_url: String, options: Value) -> Result<(), String> {
    let opts: SpiderOptions = settings::options_from_value(options);
    crawl::start_spider(app, start_url, opts).await;
    Ok(())
}

#[tauri::command]
pub fn spider_pause() -> Value {
    crawl::pause_spider()
}

#[tauri::command]
pub fn spider_resume(app: AppHandle) -> Value {
    crawl::resume_spider(app)
}

#[tauri::command]
pub fn spider_stop(app: AppHandle) {
    crawl::stop_spider(app);
}

fn default_dump_name(start_url: &str) -> String {
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

fn validate_dump(data: &Value) -> Result<(), String> {
    let obj = data.as_object().ok_or("Файл порожній або пошкоджений.")?;
    let version = obj.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != DUMP_VERSION {
        return Err(format!("Непідтримувана версія дампу: {version}"));
    }
    if !obj.get("results").map(|v| v.is_array()).unwrap_or(false) {
        return Err("У дампі немає масиву results.".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn session_save(app: AppHandle, payload: Value) -> Result<Value, String> {
    let start_url = payload
        .get("startUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let file_path = app
        .dialog()
        .file()
        .set_title("Зберегти дамп сканування")
        .set_file_name(&default_dump_name(start_url))
        .add_filter("Дамп сканування Spider", &["spider.json", "json"])
        .blocking_save_file();

    let Some(FilePath::Path(path)) = file_path else {
        return Ok(serde_json::json!({ "ok": false, "canceled": true }));
    };

    let mut dump = payload;
    if let Some(obj) = dump.as_object_mut() {
        obj.insert("version".into(), Value::from(DUMP_VERSION));
        obj.insert("app".into(), Value::from("spider-tauri"));
        obj.insert(
            "savedAt".into(),
            Value::from(chrono::Utc::now().to_rfc3339()),
        );
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&dump).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "filePath": path.to_string_lossy() }))
}

#[tauri::command]
pub async fn session_save_json(
    app: AppHandle,
    start_url: Option<String>,
    dump_json: String,
) -> Result<Value, String> {
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, dump_json).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "filePath": path.to_string_lossy() }))
}

#[tauri::command]
pub async fn session_load(app: AppHandle) -> Result<Value, String> {
    let file_path = app
        .dialog()
        .file()
        .set_title("Завантажити дамп сканування")
        .add_filter("Дамп сканування Spider", &["spider.json", "json"])
        .blocking_pick_file();

    let Some(FilePath::Path(path)) = file_path else {
        return Ok(serde_json::json!({ "ok": false, "canceled": true }));
    };
    let raw = fs::read_to_string(&path).map_err(|_| "Не вдалося прочитати JSON-файл.".to_string())?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|_| "Не вдалося прочитати JSON-файл.".to_string())?;
    validate_dump(&parsed)?;
    Ok(serde_json::json!({
        "ok": true,
        "filePath": path.to_string_lossy(),
        "dumpJson": raw
    }))
}

#[allow(dead_code)]
fn _keep(_: AppSettings) {}
