//! Serde types mirroring the JSON shapes consumed by the renderer.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// A single response header row (`{ name, value }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
}

/// A heading extracted from the page (`h1`..`h6`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

/// Metadata describing one link edge (page -> target).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkMeta {
    pub text: String,
    pub rel: String,
    pub tag: String,
    pub kind: String,
    pub rel_follow_allowed: Option<bool>,
    pub rel_index_allowed: Option<bool>,
    pub rel_label: String,
    pub img_alt_missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub img_alt: Option<String>,
}

/// One entry of the `referrers` array attached to a result row.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferrerEntry {
    pub href: String,
    pub text: String,
    pub rel: String,
    pub tag: String,
    pub kind: String,
    pub rel_follow_allowed: Option<bool>,
    pub rel_index_allowed: Option<bool>,
    pub rel_label: String,
    pub img_alt_missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub img_alt: Option<String>,
}

impl ReferrerEntry {
    pub fn from_meta(href: impl Into<String>, meta: &LinkMeta) -> Self {
        Self {
            href: href.into(),
            text: meta.text.clone(),
            rel: meta.rel.clone(),
            tag: meta.tag.clone(),
            kind: meta.kind.clone(),
            rel_follow_allowed: meta.rel_follow_allowed,
            rel_index_allowed: meta.rel_index_allowed,
            rel_label: meta.rel_label.clone(),
            img_alt_missing: meta.img_alt_missing,
            img_alt: meta.img_alt.clone(),
        }
    }
}

/// Robots.txt verdict cached per URL, mirrored into `robotsByUrl`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RobotsFields {
    pub robots_allowed: Option<bool>,
    pub robots_rule: String,
}

/// The `spider-result` payload. Field names match the Electron build exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpiderResult {
    pub url: String,
    /// Number for HTTP statuses, `""` for discovered-only stubs, `"ERROR"` on failure.
    pub status: Value,
    pub title: String,
    pub meta_description: String,
    pub meta_canonical: String,
    pub content_type: String,

    pub meta_robots: String,
    pub meta_robots_status: String,
    pub meta_robots_label: String,
    pub x_robots_tag: String,
    pub x_robots_tag_status: String,
    pub x_robots_tag_label: String,

    pub response_headers: Vec<HeaderEntry>,
    pub robots_allowed: Option<bool>,
    pub robots_rule: String,
    pub response_time_ms: Option<u64>,

    pub redirect_url: String,
    pub redirect_hop_count: u32,
    pub redirect_final_url: String,
    pub redirect_infinite: bool,
    pub redirect_chain: Vec<String>,
    pub redirect_loop_start_url: String,
    pub redirect_hop_only: bool,

    pub external: bool,
    pub fetched: bool,
    pub kind: String,
    pub tag: String,
    pub text: String,

    pub rel: String,
    pub rel_follow_allowed: Option<bool>,
    pub rel_index_allowed: Option<bool>,
    pub rel_label: String,

    pub img_alt_missing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub img_alt: Option<String>,

    pub referrers: Vec<ReferrerEntry>,
    pub headings: Vec<Heading>,

    pub og_title: String,
    pub og_description: String,
    pub og_image: String,
}

impl Default for SpiderResult {
    fn default() -> Self {
        Self {
            url: String::new(),
            status: Value::String(String::new()),
            title: String::new(),
            meta_description: String::new(),
            meta_canonical: String::new(),
            content_type: String::new(),
            meta_robots: String::new(),
            meta_robots_status: "none".to_string(),
            meta_robots_label: String::new(),
            x_robots_tag: String::new(),
            x_robots_tag_status: "none".to_string(),
            x_robots_tag_label: String::new(),
            response_headers: Vec::new(),
            robots_allowed: None,
            robots_rule: String::new(),
            response_time_ms: None,
            redirect_url: String::new(),
            redirect_hop_count: 0,
            redirect_final_url: String::new(),
            redirect_infinite: false,
            redirect_chain: Vec::new(),
            redirect_loop_start_url: String::new(),
            redirect_hop_only: false,
            external: false,
            fetched: false,
            kind: String::new(),
            tag: String::new(),
            text: String::new(),
            rel: String::new(),
            rel_follow_allowed: None,
            rel_index_allowed: None,
            rel_label: String::new(),
            img_alt_missing: false,
            img_alt: None,
            referrers: Vec::new(),
            headings: Vec::new(),
            og_title: String::new(),
            og_description: String::new(),
            og_image: String::new(),
        }
    }
}

/// The `spider-progress` payload.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub scanned: usize,
    pub queue: usize,
    pub queue_html: usize,
    pub queue_media: usize,
    pub active: usize,
    pub concurrency: usize,
    pub paused: bool,
    pub pages_per_second: f64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<bool>,
}

/// The `spider-referrers-update` payload.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferrersUpdatePayload {
    pub referrers: std::collections::HashMap<String, Vec<ReferrerEntry>>,
    pub robots_by_url: std::collections::HashMap<String, RobotsFields>,
    pub skip_full_sync: bool,
}

/// Scan options sent by the renderer with `start_spider`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpiderOptions {
    #[serde(deserialize_with = "de_bool")]
    pub use_sitemap: bool,
    #[serde(deserialize_with = "de_string_vec")]
    pub sitemap_urls: Vec<String>,
    #[serde(deserialize_with = "de_bool")]
    pub respect_robots_txt: bool,
    #[serde(deserialize_with = "de_u64")]
    pub request_delay_ms: u64,
    #[serde(deserialize_with = "de_string")]
    pub user_agent_preset: String,
    #[serde(deserialize_with = "de_string")]
    pub user_agent_custom: String,
    #[serde(deserialize_with = "de_u64")]
    pub max_pages: u64,
    #[serde(deserialize_with = "de_u64")]
    pub concurrency: u64,
    #[serde(deserialize_with = "de_string")]
    pub auth_type: String,
    #[serde(deserialize_with = "de_string")]
    pub auth_username: String,
    #[serde(deserialize_with = "de_string")]
    pub auth_password: String,
    #[serde(deserialize_with = "de_string")]
    pub auth_token: String,
}

impl Default for SpiderOptions {
    fn default() -> Self {
        Self {
            use_sitemap: false,
            sitemap_urls: Vec::new(),
            respect_robots_txt: true,
            request_delay_ms: 500,
            user_agent_preset: "spider".to_string(),
            user_agent_custom: String::new(),
            max_pages: 0,
            concurrency: 3,
            auth_type: "none".to_string(),
            auth_username: String::new(),
            auth_password: String::new(),
            auth_token: String::new(),
        }
    }
}

/// Coerce JSON booleans, numbers and strings into `bool` (the settings form
/// may hand back `"true"` / `1` depending on the widget).
pub fn value_to_bool(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|v| v != 0.0).unwrap_or(false),
        Value::String(s) => {
            let lower = s.trim().to_ascii_lowercase();
            lower == "true" || lower == "1" || lower == "yes" || lower == "on"
        }
        _ => false,
    }
}

/// Coerce JSON numbers and numeric strings into `u64`, clamping negatives to 0.
pub fn value_to_u64(value: &Value) -> u64 {
    match value {
        Value::Number(n) => n.as_f64().map(|v| if v > 0.0 { v as u64 } else { 0 }).unwrap_or(0),
        Value::String(s) => s.trim().parse::<f64>().ok().map(|v| if v > 0.0 { v as u64 } else { 0 }).unwrap_or(0),
        Value::Bool(b) => u64::from(*b),
        _ => 0,
    }
}

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn de_bool<'de, D: Deserializer<'de>>(de: D) -> Result<bool, D::Error> {
    let value = Value::deserialize(de)?;
    Ok(value_to_bool(&value))
}

fn de_u64<'de, D: Deserializer<'de>>(de: D) -> Result<u64, D::Error> {
    let value = Value::deserialize(de)?;
    Ok(value_to_u64(&value))
}

fn de_string<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    let value = Value::deserialize(de)?;
    Ok(value_to_string(&value))
}

fn de_string_vec<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
    let value = Value::deserialize(de)?;
    Ok(match value {
        Value::Array(items) => items
            .iter()
            .map(value_to_string)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Value::String(s) => s
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect(),
        _ => Vec::new(),
    })
}
