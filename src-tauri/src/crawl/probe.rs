//! Discovered-link handling: stub rows plus status-only probes for assets
//! and external pages.

use serde_json::json;
use tauri::AppHandle;

use crate::crawl::emit;
use crate::crawl::html::Link;
use crate::crawl::network::{self, client};
use crate::crawl::queue::{self, QueueItem};
use crate::crawl::redirect::RedirectTracker;
use crate::crawl::referrers;
use crate::crawl::results;
use crate::crawl::state::runtime;
use crate::crawl::types::RobotsFields;
use crate::crawl::url_utils::{is_redirect_status, resolve_redirect_target};

/// Record every link found on a page: update the referrer graph, emit a stub
/// row the first time each target is seen, and queue follow-up work.
pub async fn report_discovered_links(app: &AppHandle, page_url: &str, links: &[Link]) {
    let rt = runtime();
    let hostname = rt.hostname();

    for link in links {
        if link.url.is_empty() {
            continue;
        }
        referrers::add_referrer(&link.url, page_url, link.to_meta());

        if rt.mark_stub_reported(&link.url) && !rt.is_visited(&link.url) {
            let stub = results::build_stub_result(&link.url, &hostname, &link.to_meta(), link.external);
            emit::queue_result(app, stub);
        }

        if link.crawlable && !link.external {
            queue::enqueue_url(&link.url);
        } else if queue::needs_link_probe(link) {
            queue::enqueue_probe(&link.url, link.to_meta(), link.external);
        }
    }
}

/// Fetch just the status line and headers of a discovered resource.
pub async fn probe_discovered_link(app: &AppHandle, item: QueueItem) {
    let rt = runtime();
    let config = rt.config();

    if !rt.mark_probed(&item.url) {
        return;
    }

    let mut tracker = RedirectTracker::new(item.url.clone());
    let mut current = item.url.clone();
    let mut final_status: Option<u16> = None;
    let mut content_type = String::new();
    let mut response_time_ms: Option<u64> = None;
    let mut x_robots = String::new();
    let mut redirect_target = String::new();
    let mut error_message: Option<String> = None;

    loop {
        network::wait_before_request(config.request_delay_ms).await;

        match client()
            .probe(&current, &config.user_agent, &config.auth)
            .await
        {
            Err(error) => {
                error_message = Some(error);
                break;
            }
            Ok(response) => {
                if final_status.is_none() {
                    final_status = Some(response.status);
                    content_type = response.content_type.clone();
                    response_time_ms = Some(response.elapsed_ms);
                    x_robots = response.x_robots_tag.clone();
                }
                if !is_redirect_status(response.status) {
                    if tracker.hop_count() > 0 {
                        content_type = response.content_type.clone();
                    }
                    break;
                }
                let target = response
                    .location
                    .as_deref()
                    .and_then(|location| resolve_redirect_target(&current, location));
                let Some(target) = target else {
                    break;
                };
                if redirect_target.is_empty() {
                    redirect_target = target.clone();
                }
                if !tracker.can_follow() {
                    break;
                }
                if !tracker.record_hop(target.clone()) {
                    break;
                }
                current = target;
            }
        }
    }

    let mut result = results::build_stub_result(&item.url, &config.hostname, &item.meta, item.external);
    result.fetched = true;

    match (final_status, error_message) {
        (Some(status), _) => {
            result.status = json!(status);
            result.content_type = content_type;
            result.response_time_ms = response_time_ms;
        }
        (None, Some(message)) => {
            result.status = json!("ERROR");
            result.text = if item.meta.text.is_empty() {
                message
            } else {
                item.meta.text.clone()
            };
        }
        (None, None) => {
            result.status = json!("ERROR");
        }
    }

    let robots = if item.external {
        RobotsFields::default()
    } else {
        let fields =
            network::check_robots(&item.url, &config.user_agent, config.respect_robots).await;
        referrers::set_robots_fields(&item.url, fields.clone());
        fields
    };
    results::apply_indexing_fields(&mut result, "", &x_robots, &robots);
    tracker.to_fields(&mut result, &redirect_target);

    rt.bump_scanned();
    emit::queue_result(app, result);
}
