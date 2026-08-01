//! Sitemap discovery and parsing used to seed the crawl queue.

use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;
use tauri::AppHandle;
use tokio::task::JoinSet;
use url::Url;

use crate::crawl::auth::AuthConfig;
use crate::crawl::emit;
use crate::crawl::network::{client, robots_sitemaps, SITEMAP_TIMEOUT};
use crate::crawl::queue;
use crate::crawl::referrers;
use crate::crawl::types::{LinkMeta, ProgressPayload};
use crate::crawl::url_utils::{is_same_host, normalize_page_url};

/// Well-known locations tried when robots.txt advertises nothing.
pub const FALLBACK_SITEMAP_PATHS: &[&str] = &["/sitemap_index.xml", "/sitemap.xml", "/index.xml"];

const MAX_SITEMAP_DOCS: usize = 50;
const MAX_DEPTH: usize = 3;
const FETCH_CONCURRENCY: usize = 5;
const MAX_SEEDED_URLS: usize = 100_000;

static LOC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<loc[^>]*>(.*?)</loc>").expect("valid <loc> regex"));

fn sitemap_link_meta() -> LinkMeta {
    LinkMeta {
        text: "sitemap".into(),
        kind: "sitemap".into(),
        tag: "sitemap".into(),
        ..LinkMeta::default()
    }
}

/// Resolve custom sitemap entries (absolute or site-relative) against origin.
pub fn normalize_sitemap_url_list(raw: &[String], origin: &str) -> Vec<String> {
    let Ok(base) = Url::parse(origin) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for entry in raw {
        let trimmed = entry.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Ok(resolved) = base.join(trimmed) else {
            continue;
        };
        let href = resolved.to_string();
        if seen.insert(href.clone()) {
            result.push(href);
        }
    }
    result
}

/// Candidate sitemap documents for an origin: explicit overrides win,
/// otherwise robots.txt entries (or conventional fallbacks when robots has none).
pub async fn discover_sitemap_sources(
    origin: &str,
    user_agent: &str,
    custom_urls: &[String],
) -> Vec<String> {
    let custom = normalize_sitemap_url_list(custom_urls, origin);
    if !custom.is_empty() {
        return custom;
    }
    let mut sources = robots_sitemaps(origin, user_agent).await;
    if sources.is_empty() {
        let trimmed_origin = origin.trim_end_matches('/');
        for path in FALLBACK_SITEMAP_PATHS {
            sources.push(format!("{trimmed_origin}{path}"));
        }
    }
    dedupe(sources.into_iter())
}

fn emit_sitemap_progress(app: &AppHandle, status: &str) {
    let payload = ProgressPayload {
        scanned: 0,
        queue: queue::total_queue_len(),
        queue_html: queue::crawl_queue_len(),
        queue_media: queue::probe_queue_len(),
        active: 0,
        concurrency: 0,
        paused: false,
        pages_per_second: 0.0,
        status: status.to_string(),
        finished: None,
    };
    emit::emit_progress(app, &payload);
}

/// Fetch the sitemap tree, enqueue same-host page URLs, and stream progress
/// (Electron parity: status text updates as each leaf sitemap is read).
///
/// Returns the number of unique page URLs seeded into the queue.
pub async fn seed_queue_from_sitemaps(
    app: &AppHandle,
    origin: &str,
    hostname: &str,
    user_agent: &str,
    auth: &AuthConfig,
    custom_urls: &[String],
    aborting: impl Fn() -> bool,
) -> usize {
    let sources = discover_sitemap_sources(origin, user_agent, custom_urls).await;
    if aborting() {
        return 0;
    }

    emit_sitemap_progress(app, &format!("Пошук sitemap ({})...", sources.len()));

    let mut visited_docs: HashSet<String> = HashSet::new();
    let mut pending: Vec<String> = sources;
    let mut seen_pages: HashSet<String> = HashSet::new();
    let mut seeded = 0usize;
    let mut leaf_files_done = 0usize;
    let mut depth = 0usize;
    let meta = sitemap_link_meta();

    while !pending.is_empty() && depth < MAX_DEPTH && visited_docs.len() < MAX_SITEMAP_DOCS {
        if aborting() {
            break;
        }
        let mut next_level: Vec<String> = Vec::new();

        for chunk in pending.chunks(FETCH_CONCURRENCY) {
            if aborting() {
                break;
            }
            let mut tasks: JoinSet<Option<(String, bool, Vec<String>)>> = JoinSet::new();
            for url in chunk {
                if !visited_docs.insert(url.clone()) || visited_docs.len() > MAX_SITEMAP_DOCS {
                    continue;
                }
                let url = url.clone();
                let user_agent = user_agent.to_string();
                let auth = auth.clone();
                tasks.spawn(async move {
                    let (is_index, locations) =
                        fetch_sitemap_document(&url, &user_agent, &auth).await?;
                    Some((url, is_index, locations))
                });
            }

            while let Some(joined) = tasks.join_next().await {
                if aborting() {
                    break;
                }
                let Ok(Some((sitemap_url, is_index, locations))) = joined else {
                    continue;
                };
                if is_index {
                    next_level.extend(locations);
                    continue;
                }

                leaf_files_done += 1;
                for location in locations {
                    if seeded >= MAX_SEEDED_URLS {
                        break;
                    }
                    let Some(normalized) = normalize_page_url(&location) else {
                        continue;
                    };
                    if !is_same_host(&normalized, hostname) {
                        continue;
                    }
                    if !seen_pages.insert(normalized.clone()) {
                        continue;
                    }
                    referrers::add_referrer(&normalized, &sitemap_url, meta.clone());
                    if queue::enqueue_url(&normalized) {
                        seeded += 1;
                    }
                }
                emit_sitemap_progress(
                    app,
                    &format!("Sitemap {leaf_files_done}: у черзі {}", queue::total_queue_len()),
                );
                if seeded >= MAX_SEEDED_URLS {
                    return seeded;
                }
            }
        }

        pending = next_level;
        depth += 1;
    }

    seeded
}

/// Returns `(is_sitemap_index, locations)` for one sitemap document.
async fn fetch_sitemap_document(
    url: &str,
    user_agent: &str,
    auth: &AuthConfig,
) -> Option<(bool, Vec<String>)> {
    let (status, body) = client()
        .fetch_text(url, user_agent, auth, SITEMAP_TIMEOUT)
        .await
        .ok()?;
    if !(200..300).contains(&status) || body.trim().is_empty() {
        return None;
    }
    Some(parse_sitemap(&body))
}

/// Extract `<loc>` values and detect whether the document is an index.
pub fn parse_sitemap(body: &str) -> (bool, Vec<String>) {
    let is_index = body.to_ascii_lowercase().contains("<sitemapindex");
    let locations = LOC_RE
        .captures_iter(body)
        .filter_map(|capture| capture.get(1))
        .map(|matched| decode_xml_entities(matched.as_str().trim()))
        .filter(|value| value.starts_with("http"))
        .collect();
    (is_index, locations)
}

fn decode_xml_entities(raw: &str) -> String {
    let cleaned = raw
        .trim()
        .trim_start_matches("<![CDATA[")
        .trim_end_matches("]]>")
        .trim();
    cleaned
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn dedupe(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        result.push(trimmed);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_relative_sitemap_paths() {
        let urls = normalize_sitemap_url_list(
            &["/custom-sitemap.xml".into(), "https://ex.com/a.xml".into()],
            "https://ex.com",
        );
        assert_eq!(
            urls,
            vec![
                "https://ex.com/custom-sitemap.xml".to_string(),
                "https://ex.com/a.xml".to_string(),
            ]
        );
    }

    #[test]
    fn parse_urlset_and_index() {
        let (is_index, locs) = parse_sitemap(
            r#"<?xml version="1.0"?>
            <urlset><url><loc>https://ex.com/a</loc></url></urlset>"#,
        );
        assert!(!is_index);
        assert_eq!(locs, vec!["https://ex.com/a".to_string()]);

        let (is_index, locs) = parse_sitemap(
            r#"<sitemapindex><sitemap><loc>https://ex.com/s1.xml</loc></sitemap></sitemapindex>"#,
        );
        assert!(is_index);
        assert_eq!(locs, vec!["https://ex.com/s1.xml".to_string()]);
    }
}
