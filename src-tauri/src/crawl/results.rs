//! Construction of `spider-result` rows.

use serde_json::Value;

use crate::crawl::types::{LinkMeta, RobotsFields, SpiderResult};
use crate::crawl::url_utils::is_same_host;

/// Empty row for `url`. Referrers are omitted here — the full graph is
/// shipped once at scan end via `spider-referrers-update` (avoids double IPC).
pub fn build_spider_result(url: &str, hostname: &str) -> SpiderResult {
    SpiderResult {
        url: url.to_string(),
        external: !is_same_host(url, hostname),
        ..SpiderResult::default()
    }
}

/// Discovered-but-not-fetched row (`status: ""`, `fetched: false`).
pub fn build_stub_result(url: &str, hostname: &str, meta: &LinkMeta, external: bool) -> SpiderResult {
    let mut result = build_spider_result(url, hostname);
    result.status = Value::String(String::new());
    result.fetched = false;
    result.external = external;
    result.kind = meta.kind.clone();
    result.tag = meta.tag.clone();
    result.text = meta.text.clone();
    result.rel = meta.rel.clone();
    result.rel_follow_allowed = meta.rel_follow_allowed;
    result.rel_index_allowed = meta.rel_index_allowed;
    result.rel_label = meta.rel_label.clone();
    result.img_alt_missing = meta.img_alt_missing;
    result.img_alt = meta.img_alt.clone();
    result
}

/// Row for a request that never produced a response.
pub fn build_error_result(url: &str, hostname: &str, message: &str) -> SpiderResult {
    let mut result = build_spider_result(url, hostname);
    result.status = Value::String("ERROR".to_string());
    result.fetched = true;
    result.text = message.to_string();
    result
}

/// Split a `robots` directive into a `(status, label)` pair.
///
/// `status` is one of `none` / `allowed` / `noindex` / `nofollow`, matching the
/// values the renderer checks in `isMetaRobotsBlocked`.
pub fn parse_meta_robots_directive(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ("none".to_string(), String::new());
    }
    let tokens: Vec<String> = trimmed
        .split([',', ' ', '\t', '\n'])
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect();

    let label = trimmed.to_string();
    if tokens.iter().any(|t| t == "noindex" || t == "none") {
        return ("noindex".to_string(), label);
    }
    if tokens.iter().any(|t| t == "nofollow") {
        return ("nofollow".to_string(), label);
    }
    ("allowed".to_string(), label)
}

/// Apply meta robots, `X-Robots-Tag` and robots.txt verdicts to a row.
pub fn apply_indexing_fields(
    result: &mut SpiderResult,
    meta_robots_raw: &str,
    x_robots_raw: &str,
    robots: &RobotsFields,
) {
    let (meta_status, meta_label) = parse_meta_robots_directive(meta_robots_raw);
    result.meta_robots = meta_robots_raw.trim().to_string();
    result.meta_robots_status = meta_status;
    result.meta_robots_label = meta_label;

    let (x_status, x_label) = parse_meta_robots_directive(x_robots_raw);
    result.x_robots_tag = x_robots_raw.trim().to_string();
    result.x_robots_tag_status = x_status;
    result.x_robots_tag_label = x_label;

    result.robots_allowed = robots.robots_allowed;
    result.robots_rule = robots.robots_rule.clone();
}

/// The full `X-Robots-Tag` value, joining repeated headers with `, `.
pub fn join_x_robots_values(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_noindex() {
        let (status, _) = parse_meta_robots_directive("noindex, follow");
        assert_eq!(status, "noindex");
    }

    #[test]
    fn meta_nofollow() {
        let (status, _) = parse_meta_robots_directive("nofollow");
        assert_eq!(status, "nofollow");
    }
}
