//! Import session dumps in Rust and stream compact batches to the webview.
//! Avoids `JSON.parse` of 300MB+ files inside WebKit/JSC (multi-GB peak).

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::{DialogExt, FilePath};

const DUMP_VERSION: u64 = 1;
const BATCH_SIZE: usize = 150;

fn deserialize_referrers<'de, D>(deserializer: D) -> Result<Vec<ImportReferrer>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Vec<Value> = Vec::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(raw.len());
    for item in raw {
        if let Some(href) = item.as_str() {
            out.push(ImportReferrer {
                href: href.to_string(),
                ..ImportReferrer::default()
            });
            continue;
        }
        if let Ok(entry) = serde_json::from_value::<ImportReferrer>(item) {
            if !entry.href.is_empty() {
                out.push(entry);
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReferrer {
    #[serde(default)]
    href: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    rel: String,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    rel_follow_allowed: Option<bool>,
    #[serde(default)]
    rel_index_allowed: Option<bool>,
    #[serde(default)]
    rel_label: String,
    #[serde(default)]
    img_alt_missing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    img_alt: Option<String>,
    // imgAltStates intentionally omitted — ignored on deserialize, never sent to JS.
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportHeading {
    level: u8,
    #[serde(default)]
    text: String,
}

/// Compact dump row: heavy Electron fields (responseHeaders, redirectChain) are
/// omitted from the struct so serde drops them while parsing.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportEntry {
    url: String,
    #[serde(default)]
    status: Value,
    #[serde(default)]
    title: String,
    #[serde(default)]
    meta_description: String,
    #[serde(default)]
    meta_canonical: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    meta_robots: String,
    #[serde(default)]
    meta_robots_status: String,
    #[serde(default)]
    meta_robots_label: String,
    #[serde(default)]
    x_robots_tag: String,
    #[serde(default)]
    x_robots_tag_status: String,
    #[serde(default)]
    x_robots_tag_label: String,
    #[serde(default)]
    robots_allowed: Option<bool>,
    #[serde(default)]
    robots_rule: String,
    #[serde(default)]
    response_time_ms: Option<u64>,
    #[serde(default)]
    redirect_url: String,
    #[serde(default)]
    redirect_hop_count: u32,
    #[serde(default)]
    redirect_final_url: String,
    #[serde(default)]
    redirect_infinite: bool,
    #[serde(default)]
    redirect_loop_start_url: String,
    #[serde(default)]
    redirect_hop_only: bool,
    #[serde(default)]
    external: bool,
    #[serde(default)]
    fetched: Option<bool>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    img_alt_missing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    img_alt: Option<String>,
    #[serde(default, deserialize_with = "deserialize_referrers")]
    referrers: Vec<ImportReferrer>,
    #[serde(default)]
    headings: Vec<ImportHeading>,
    #[serde(default)]
    og_title: String,
    #[serde(default)]
    og_description: String,
    #[serde(default)]
    og_image: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DumpFile {
    version: u64,
    #[serde(default)]
    start_url: String,
    #[serde(default)]
    progress_at_save: Option<Value>,
    #[serde(default)]
    settings: Option<Value>,
    #[serde(default)]
    insertion_order: Vec<String>,
    #[serde(default)]
    results: Vec<ImportEntry>,
}

fn parse_dump(path: &std::path::Path) -> Result<DumpFile, String> {
    let file = File::open(path).map_err(|_| "Не вдалося прочитати JSON-файл.".to_string())?;
    let reader = BufReader::with_capacity(1024 * 1024, file);
    let dump: DumpFile =
        serde_json::from_reader(reader).map_err(|_| "Не вдалося розібрати файл дампу.".to_string())?;
    if dump.version != DUMP_VERSION {
        return Err(format!("Непідтримувана версія дампу: {}", dump.version));
    }
    if dump.results.is_empty() {
        return Err("У дампі немає масиву results.".into());
    }
    Ok(dump)
}

/// Pick a dump file, parse in Rust (dropping headers/chains), stream batches to JS.
#[tauri::command]
pub async fn session_import(app: AppHandle) -> Result<Value, String> {
    let file_path = app
        .dialog()
        .file()
        .set_title("Завантажити дамп сканування")
        .add_filter("Дамп сканування Spider", &["spider.json", "json"])
        .blocking_pick_file();

    let Some(FilePath::Path(path)) = file_path else {
        return Ok(serde_json::json!({ "ok": false, "canceled": true }));
    };
    if !path.is_file() {
        return Err("Не вдалося прочитати JSON-файл.".into());
    }

    let path_display = path.to_string_lossy().to_string();
    let app_parse = app.clone();
    let dump = tauri::async_runtime::spawn_blocking(move || parse_dump(&path))
        .await
        .map_err(|e| e.to_string())??;

    let result_count = dump.results.len();
    let insertion_order = if dump.insertion_order.is_empty() {
        dump.results.iter().map(|e| e.url.clone()).collect::<Vec<_>>()
    } else {
        dump.insertion_order
    };

    let _ = app_parse.emit(
        "session-dump-import-start",
        serde_json::json!({
            "filePath": path_display,
            "startUrl": dump.start_url,
            "progressAtSave": dump.progress_at_save,
            "settings": dump.settings,
            "insertionOrder": insertion_order,
            "resultCount": result_count,
        }),
    );

    let mut results = dump.results;
    while !results.is_empty() {
        let take = BATCH_SIZE.min(results.len());
        let batch: Vec<ImportEntry> = results.drain(..take).collect();
        let _ = app_parse.emit(
            "session-dump-import-batch",
            serde_json::json!({ "entries": batch }),
        );
        // Let the webview apply + GC between chunks (yield alone is not enough on Linux).
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    }

    let _ = app_parse.emit(
        "session-dump-import-done",
        serde_json::json!({ "ok": true, "resultCount": result_count }),
    );

    Ok(serde_json::json!({
        "ok": true,
        "resultCount": result_count,
    }))
}
