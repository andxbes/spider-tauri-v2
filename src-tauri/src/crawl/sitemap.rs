//! Sitemap discovery and parsing used to seed the crawl queue.

use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;
use tokio::task::JoinSet;

use crate::crawl::auth::AuthConfig;
use crate::crawl::network::{client, robots_sitemaps, SITEMAP_TIMEOUT};
use crate::crawl::url_utils::normalize_page_url;

/// Well-known locations tried when robots.txt advertises nothing.
pub const FALLBACK_SITEMAP_PATHS: &[&str] = &["/sitemap_index.xml", "/sitemap.xml", "/index.xml"];

const MAX_SITEMAP_DOCS: usize = 50;
const MAX_DEPTH: usize = 3;
const FETCH_CONCURRENCY: usize = 5;
const MAX_SEEDED_URLS: usize = 100_000;

static LOC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<loc[^>]*>(.*?)</loc>").expect("valid <loc> regex"));

/// Candidate sitemap documents for an origin: explicit overrides win,
/// otherwise robots.txt entries plus the conventional fallbacks.
pub async fn discover_sitemap_sources(
    origin: &str,
    user_agent: &str,
    custom_urls: &[String],
) -> Vec<String> {
    if !custom_urls.is_empty() {
        return dedupe(custom_urls.iter().cloned());
    }
    let mut sources = robots_sitemaps(origin, user_agent).await;
    let trimmed_origin = origin.trim_end_matches('/');
    for path in FALLBACK_SITEMAP_PATHS {
        sources.push(format!("{trimmed_origin}{path}"));
    }
    dedupe(sources.into_iter())
}

/// Fetch the sitemap tree and return every page URL it lists.
pub async fn discover_sitemap_urls(
    origin: &str,
    user_agent: &str,
    auth: &AuthConfig,
    custom_urls: &[String],
) -> Vec<String> {
    let sources = discover_sitemap_sources(origin, user_agent, custom_urls).await;
    let mut visited_docs: HashSet<String> = HashSet::new();
    let mut pending: Vec<String> = sources;
    let mut page_urls: Vec<String> = Vec::new();
    let mut seen_pages: HashSet<String> = HashSet::new();
    let mut depth = 0usize;

    while !pending.is_empty() && depth < MAX_DEPTH && visited_docs.len() < MAX_SITEMAP_DOCS {
        let mut next_level: Vec<String> = Vec::new();

        for chunk in pending.chunks(FETCH_CONCURRENCY) {
            let mut tasks: JoinSet<Option<(bool, Vec<String>)>> = JoinSet::new();
            for url in chunk {
                if !visited_docs.insert(url.clone()) || visited_docs.len() > MAX_SITEMAP_DOCS {
                    continue;
                }
                let url = url.clone();
                let user_agent = user_agent.to_string();
                let auth = auth.clone();
                tasks.spawn(async move { fetch_sitemap_document(&url, &user_agent, &auth).await });
            }

            while let Some(joined) = tasks.join_next().await {
                let Ok(Some((is_index, locations))) = joined else {
                    continue;
                };
                for location in locations {
                    if is_index {
                        next_level.push(location);
                    } else if let Some(normalized) = normalize_page_url(&location) {
                        if seen_pages.insert(normalized.clone()) {
                            page_urls.push(normalized);
                        }
                    }
                }
            }

            if page_urls.len() >= MAX_SEEDED_URLS {
                return page_urls;
            }
        }

        pending = next_level;
        depth += 1;
    }

    page_urls
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
