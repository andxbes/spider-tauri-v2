//! Inbound-link graph: `target URL -> { referrer URL -> edge metadata }`.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::crawl::types::{LinkMeta, ReferrerEntry, ReferrersUpdatePayload, RobotsFields};

/// Above this many tracked targets the full graph is too big to ship over IPC,
/// so the renderer is told to rebuild it locally instead.
pub const FULL_SYNC_LIMIT: usize = 8000;

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

/// Inbound links for one target, ready to attach to a result row.
pub fn get_list(target: &str) -> Vec<ReferrerEntry> {
    let store = STORE.lock();
    store
        .by_target
        .get(target)
        .map(|edges| {
            edges
                .iter()
                .map(|(href, meta)| ReferrerEntry::from_meta(href.clone(), meta))
                .collect()
        })
        .unwrap_or_default()
}

pub fn target_count() -> usize {
    STORE.lock().by_target.len()
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

/// Build the `spider-referrers-update` payload, skipping the (potentially
/// huge) graph when there are too many targets.
pub fn build_all_payload() -> ReferrersUpdatePayload {
    let count = target_count();
    if count > FULL_SYNC_LIMIT {
        return ReferrersUpdatePayload {
            referrers: HashMap::new(),
            robots_by_url: HashMap::new(),
            skip_full_sync: true,
        };
    }
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
