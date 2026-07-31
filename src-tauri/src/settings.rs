use crate::crawl::types::{value_to_bool, value_to_string, value_to_u64, SpiderOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub const DEFAULT_CONCURRENCY: u64 = 3;
pub const MAX_CONCURRENCY: u64 = 50;
pub const DEFAULT_REQUEST_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub use_sitemap: bool,
    pub max_pages: u64,
    pub concurrency: u64,
    pub respect_robots_txt: bool,
    pub request_delay_ms: u64,
    pub user_agent_preset: String,
    pub user_agent_custom: String,
    pub auth_type: String,
    pub auth_username: String,
    pub auth_password: String,
    pub auth_token: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            use_sitemap: false,
            max_pages: 0,
            concurrency: DEFAULT_CONCURRENCY,
            respect_robots_txt: true,
            request_delay_ms: DEFAULT_REQUEST_DELAY_MS,
            user_agent_preset: "spider".into(),
            user_agent_custom: String::new(),
            auth_type: "none".into(),
            auth_username: String::new(),
            auth_password: String::new(),
            auth_token: String::new(),
        }
    }
}

impl AppSettings {
    pub fn normalize(raw: &Value) -> Self {
        let mut s = Self::default();
        if let Some(v) = raw.get("useSitemap") {
            s.use_sitemap = value_to_bool(v);
        }
        if let Some(v) = raw.get("maxPages") {
            s.max_pages = value_to_u64(v);
        }
        if let Some(v) = raw.get("concurrency") {
            s.concurrency = value_to_u64(v).clamp(1, MAX_CONCURRENCY);
        }
        if let Some(v) = raw.get("respectRobotsTxt") {
            s.respect_robots_txt = value_to_bool(v);
        }
        if let Some(v) = raw.get("requestDelayMs") {
            s.request_delay_ms = value_to_u64(v).min(60_000);
        }
        if let Some(v) = raw.get("userAgentPreset") {
            s.user_agent_preset = value_to_string(v);
        }
        if let Some(v) = raw.get("userAgentCustom") {
            s.user_agent_custom = value_to_string(v);
        }
        if let Some(v) = raw.get("authType") {
            let t = value_to_string(v).to_ascii_lowercase();
            s.auth_type = if matches!(t.as_str(), "basic" | "bearer" | "none") {
                t
            } else {
                "none".into()
            };
        }
        if let Some(v) = raw.get("authUsername") {
            s.auth_username = value_to_string(v).trim().to_string();
        }
        if let Some(v) = raw.get("authPassword") {
            s.auth_password = value_to_string(v);
        }
        if let Some(v) = raw.get("authToken") {
            s.auth_token = value_to_string(v).trim().to_string();
        }
        s
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load_settings(app: &AppHandle) -> Result<(AppSettings, String), String> {
    let path = settings_path(app)?;
    let path_str = path.to_string_lossy().to_string();
    if !path.exists() {
        return Ok((AppSettings::default(), path_str));
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let raw: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok((AppSettings::normalize(&raw), path_str))
}

pub fn save_settings(app: &AppHandle, settings: Value) -> Result<(AppSettings, String), String> {
    let normalized = AppSettings::normalize(&settings);
    let path = settings_path(app)?;
    let path_str = path.to_string_lossy().to_string();
    let text = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok((normalized, path_str))
}

pub fn options_from_value(raw: Value) -> SpiderOptions {
    let s = AppSettings::normalize(&raw);
    let mut opts = SpiderOptions {
        use_sitemap: s.use_sitemap,
        respect_robots_txt: s.respect_robots_txt,
        request_delay_ms: s.request_delay_ms,
        user_agent_preset: s.user_agent_preset,
        user_agent_custom: s.user_agent_custom,
        max_pages: s.max_pages,
        concurrency: s.concurrency,
        auth_type: s.auth_type,
        auth_username: s.auth_username,
        auth_password: s.auth_password,
        auth_token: s.auth_token,
        sitemap_urls: vec![],
    };
    if let Some(arr) = raw.get("sitemapUrls").and_then(|v| v.as_array()) {
        opts.sitemap_urls = arr.iter().map(value_to_string).filter(|s| !s.is_empty()).collect();
    } else if let Some(s) = raw.get("sitemapUrls").and_then(|v| v.as_str()) {
        opts.sitemap_urls = s
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
    }
    if let Some(v) = raw.get("useSitemap") {
        opts.use_sitemap = value_to_bool(v);
    }
    opts
}
