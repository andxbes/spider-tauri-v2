//! Two-tier work queue: HTML pages first, discovered assets afterwards.

use crate::crawl::html::Link;
use crate::crawl::state::runtime;
use crate::crawl::types::LinkMeta;
use crate::crawl::url_utils::normalize_page_url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    /// Fetch and parse an internal HTML page.
    Crawl,
    /// Fetch only the status/headers of a discovered resource.
    Probe,
}

#[derive(Debug, Clone)]
pub struct QueueItem {
    pub url: String,
    pub kind: QueueKind,
    pub meta: LinkMeta,
    /// Set for probe items pointing outside the scanned host.
    pub external: bool,
}

/// Queue an internal page for a full crawl. Returns `false` if it is a
/// duplicate, already visited, or the page budget is exhausted.
pub fn enqueue_url(raw_url: &str) -> bool {
    let Some(url) = normalize_page_url(raw_url) else {
        return false;
    };
    let rt = runtime();
    if rt.is_visited(&url) || rt.page_limit_reached() {
        return false;
    }
    if !rt.queued.lock().insert(url.clone()) {
        return false;
    }
    rt.crawl_queue.lock().push_back(QueueItem {
        url,
        kind: QueueKind::Crawl,
        meta: LinkMeta::default(),
        external: false,
    });
    true
}

/// Queue a discovered resource for a status-only probe.
pub fn enqueue_probe(raw_url: &str, meta: LinkMeta, external: bool) -> bool {
    let Some(url) = normalize_page_url(raw_url) else {
        return false;
    };
    let rt = runtime();
    if rt.is_visited(&url) || rt.is_probed(&url) {
        return false;
    }
    if !rt.queued.lock().insert(url.clone()) {
        return false;
    }
    rt.probe_queue.lock().push_back(QueueItem {
        url,
        kind: QueueKind::Probe,
        meta,
        external,
    });
    true
}

/// Pop the next unit of work: crawl items win until the page budget is hit.
pub fn dequeue_next() -> Option<QueueItem> {
    let rt = runtime();
    let item = if rt.page_limit_reached() {
        rt.probe_queue.lock().pop_front()
    } else {
        let next = rt.crawl_queue.lock().pop_front();
        match next {
            Some(item) => Some(item),
            None => rt.probe_queue.lock().pop_front(),
        }
    };
    if let Some(item) = &item {
        rt.queued.lock().remove(&item.url);
    }
    item
}

pub fn crawl_queue_len() -> usize {
    runtime().crawl_queue.lock().len()
}

pub fn probe_queue_len() -> usize {
    runtime().probe_queue.lock().len()
}

pub fn total_queue_len() -> usize {
    crawl_queue_len() + probe_queue_len()
}

pub fn has_pending_work() -> bool {
    if runtime().page_limit_reached() {
        return probe_queue_len() > 0;
    }
    total_queue_len() > 0
}

/// Discovered links that are not crawlable still deserve a status code:
/// external pages, images, scripts, stylesheets, documents and so on.
pub fn needs_link_probe(link: &Link) -> bool {
    if link.url.is_empty() {
        return false;
    }
    if link.crawlable && !link.external {
        return false;
    }
    let rt = runtime();
    !rt.is_visited(&link.url) && !rt.is_probed(&link.url)
}
