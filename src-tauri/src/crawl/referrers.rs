//! Inbound-link graph: `target URL -> { referrer URL -> edge metadata }`.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::crawl::types::{LinkMeta, ReferrerEntry, ReferrersUpdatePayload, RobotsFields};

struct ReferrerStore {
    by_target: HashMap<String, HashMap<String, LinkMeta>>,
    robots_by_url: HashMap<String, RobotsFields>,
}

static STORE: Lazy<Mutex<ReferrerStore>> = Lazy::new(|| {
    Mutex::new(ReferrerStore {
        by_target: HashMap::new(),
        robots_by_url: HashMap::new(),
    })
});

/// Record that `referrer` links to `target`. Repeated edges merge, keeping the
/// richest metadata seen (an `img` edge without `alt` wins over one with).
pub fn add_referrer(target: &str, referrer: &str, meta: LinkMeta) {
    if target.is_empty() || referrer.is_empty() {
        return;
    }
    let mut store = STORE.lock();
    let entry = store
        .by_target
        .entry(target.to_string())
        .or_default()
        .entry(referrer.to_string())
        .or_default();

    if entry.text.is_empty() && !meta.text.is_empty() {
        entry.text = meta.text;
    }
    if entry.tag.is_empty() && !meta.tag.is_empty() {
        entry.tag = meta.tag;
    }
    if entry.kind.is_empty() && !meta.kind.is_empty() {
        entry.kind = meta.kind;
    }
    if entry.rel.is_empty() && !meta.rel.is_empty() {
        entry.rel = meta.rel;
    }
    if entry.rel_label.is_empty() && !meta.rel_label.is_empty() {
        entry.rel_label = meta.rel_label;
    }
    if entry.rel_follow_allowed.is_none() {
        entry.rel_follow_allowed = meta.rel_follow_allowed;
    }
    if entry.rel_index_allowed.is_none() {
        entry.rel_index_allowed = meta.rel_index_allowed;
    }
    entry.img_alt_missing = entry.img_alt_missing || meta.img_alt_missing;
    if entry.img_alt.is_none() {
        entry.img_alt = meta.img_alt;
    }
}

pub fn set_robots_fields(url: &str, fields: RobotsFields) {
    if url.is_empty() {
        return;
    }
    STORE.lock().robots_by_url.insert(url.to_string(), fields);
}

/// Full snapshot of the graph keyed by target URL.
pub fn snapshot() -> HashMap<String, Vec<ReferrerEntry>> {
    let store = STORE.lock();
    store
        .by_target
        .iter()
        .map(|(target, edges)| {
            let list = edges
                .iter()
                .map(|(href, meta)| ReferrerEntry::from_meta(href.clone(), meta))
                .collect::<Vec<_>>();
            (target.clone(), list)
        })
        .collect()
}

/// Build the end-of-scan `spider-referrers-update` payload.
///
/// Always ships the full graph: live `spider-result` rows no longer embed
/// referrers (that was a second copy over IPC). `skip_full_sync` stays false
/// for renderer compatibility.
pub fn build_all_payload() -> ReferrersUpdatePayload {
    let robots_by_url = STORE.lock().robots_by_url.clone();
    ReferrersUpdatePayload {
        referrers: snapshot(),
        robots_by_url,
        skip_full_sync: false,
    }
}

pub fn clear() {
    let mut store = STORE.lock();
    store.by_target.clear();
    store.robots_by_url.clear();
}
