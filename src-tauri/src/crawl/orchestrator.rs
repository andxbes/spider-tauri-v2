//! Crawl session orchestration: start / pause / resume / stop + worker pump.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::json;
use tauri::AppHandle;

use crate::crawl::auth::AuthConfig;
use crate::crawl::emit;
use crate::crawl::html::parse_html_document;
use crate::crawl::network::{self, client};
use crate::crawl::probe::{probe_discovered_link, report_discovered_links};
use crate::crawl::queue::{self, QueueItem, QueueKind};
use crate::crawl::redirect::RedirectTracker;
use crate::crawl::referrers;
use crate::crawl::results;
use crate::crawl::sitemap;
use crate::crawl::state::{runtime, SpiderConfig};
use crate::crawl::types::{ProgressPayload, SpiderOptions};
use crate::crawl::url_utils::{
    get_origin, hostname_of, is_html_content, is_redirect_status, is_same_host, normalize_page_url,
    resolve_redirect_target,
};
use crate::crawl::user_agent::resolve_user_agent;

struct SessionControl {
    paused: AtomicBool,
    stopped: AtomicBool,
    finished: AtomicBool,
    active: AtomicUsize,
    concurrency: AtomicUsize,
    scan_start: Mutex<Option<Instant>>,
    paused_at: Mutex<Option<Instant>>,
    total_paused_ms: Mutex<u64>,
}

impl Default for SessionControl {
    fn default() -> Self {
        Self {
            paused: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            concurrency: AtomicUsize::new(3),
            scan_start: Mutex::new(None),
            paused_at: Mutex::new(None),
            total_paused_ms: Mutex::new(0),
        }
    }
}

static CTRL: Lazy<SessionControl> = Lazy::new(SessionControl::default);

fn aborting() -> bool {
    CTRL.stopped.load(Ordering::SeqCst) || CTRL.finished.load(Ordering::SeqCst)
}

fn active_elapsed_ms() -> u64 {
    let Some(start) = *CTRL.scan_start.lock() else {
        return 0;
    };
    let mut ms = start.elapsed().as_millis() as u64;
    ms = ms.saturating_sub(*CTRL.total_paused_ms.lock());
    if let Some(paused_at) = *CTRL.paused_at.lock() {
        ms = ms.saturating_sub(paused_at.elapsed().as_millis() as u64);
    }
    ms
}

fn pages_per_second() -> f64 {
    let pages = runtime().scanned_count() as f64;
    let secs = active_elapsed_ms() as f64 / 1000.0;
    if secs <= 0.0 {
        0.0
    } else {
        pages / secs
    }
}

fn send_progress(app: &AppHandle, status: &str, finished: Option<bool>) {
    let payload = ProgressPayload {
        scanned: runtime().scanned_count(),
        queue: queue::total_queue_len(),
        queue_html: queue::crawl_queue_len(),
        queue_media: queue::probe_queue_len(),
        active: CTRL.active.load(Ordering::SeqCst),
        concurrency: CTRL.concurrency.load(Ordering::SeqCst),
        paused: CTRL.paused.load(Ordering::SeqCst),
        pages_per_second: pages_per_second(),
        status: status.to_string(),
        finished,
    };
    emit::emit_progress(app, &payload);
}

pub fn pause_spider() -> serde_json::Value {
    if CTRL.finished.load(Ordering::SeqCst) || CTRL.stopped.load(Ordering::SeqCst) {
        return json!({ "ok": false });
    }
    CTRL.paused.store(true, Ordering::SeqCst);
    *CTRL.paused_at.lock() = Some(Instant::now());
    json!({ "ok": true })
}

pub fn resume_spider(app: AppHandle) -> serde_json::Value {
    if CTRL.finished.load(Ordering::SeqCst)
        || CTRL.stopped.load(Ordering::SeqCst)
        || !CTRL.paused.load(Ordering::SeqCst)
    {
        return json!({ "ok": false });
    }
    CTRL.paused.store(false, Ordering::SeqCst);
    if let Some(at) = CTRL.paused_at.lock().take() {
        *CTRL.total_paused_ms.lock() += at.elapsed().as_millis() as u64;
    }
    send_progress(&app, "В процесі...", None);
    schedule_pump(app);
    json!({ "ok": true })
}

pub fn stop_spider(app: AppHandle) {
    CTRL.stopped.store(true, Ordering::SeqCst);
    CTRL.paused.store(false, Ordering::SeqCst);
    *CTRL.paused_at.lock() = None;
    schedule_finish_check(app);
}

pub async fn start_spider(app: AppHandle, start_url: String, options: SpiderOptions) {
    CTRL.stopped.store(true, Ordering::SeqCst);
    for _ in 0..100 {
        if CTRL.active.load(Ordering::SeqCst) == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let Some(start_norm) = normalize_page_url(&start_url) else {
        emit::emit_end(&app, "Помилка: невірний URL");
        return;
    };
    let Some(hostname) = hostname_of(&start_norm) else {
        emit::emit_end(&app, "Помилка: невірний URL");
        return;
    };
    let origin = get_origin(&start_norm);
    let concurrency = (options.concurrency as usize).clamp(1, 50);
    let user_agent = resolve_user_agent(&options.user_agent_preset, &options.user_agent_custom);
    let auth = AuthConfig::new(
        &options.auth_type,
        &options.auth_username,
        &options.auth_password,
        &options.auth_token,
        &hostname,
    );

    runtime().clear();
    referrers::clear();
    emit::clear_buffer();
    network::reset_throttle();

    runtime().set_config(SpiderConfig {
        start_url: start_norm.clone(),
        hostname: hostname.clone(),
        origin: origin.clone(),
        user_agent: user_agent.clone(),
        auth: auth.clone(),
        request_delay_ms: options.request_delay_ms.min(60_000),
        concurrency,
        max_pages: options.max_pages as usize,
        respect_robots: options.respect_robots_txt,
    });

    CTRL.paused.store(false, Ordering::SeqCst);
    CTRL.stopped.store(false, Ordering::SeqCst);
    CTRL.finished.store(false, Ordering::SeqCst);
    CTRL.active.store(0, Ordering::SeqCst);
    CTRL.concurrency.store(concurrency, Ordering::SeqCst);
    *CTRL.scan_start.lock() = Some(Instant::now());
    *CTRL.total_paused_ms.lock() = 0;
    *CTRL.paused_at.lock() = None;

    let app_for_flush = app.clone();
    emit::spawn_flusher(app_for_flush, || !CTRL.finished.load(Ordering::SeqCst));

    if options.use_sitemap {
        send_progress(&app, "Sitemap...", None);
        let pages = sitemap::discover_sitemap_urls(
            &origin,
            &user_agent,
            &auth,
            &options.sitemap_urls,
        )
        .await;
        for page in pages {
            if aborting() {
                break;
            }
            if is_same_host(&page, &hostname) {
                queue::enqueue_url(&page);
            }
        }
    }

    queue::enqueue_url(&start_norm);
    send_progress(&app, "В процесі...", None);
    schedule_pump(app);
}

fn schedule_pump(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        pump_once(app).await;
    });
}

fn schedule_finish_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        finish_or_continue(app);
    });
}

async fn pump_once(app: AppHandle) {
    loop {
        if CTRL.finished.load(Ordering::SeqCst) {
            return;
        }
        if CTRL.paused.load(Ordering::SeqCst) || CTRL.stopped.load(Ordering::SeqCst) {
            finish_or_continue(app);
            return;
        }

        let concurrency = CTRL.concurrency.load(Ordering::SeqCst);
        if CTRL.active.load(Ordering::SeqCst) >= concurrency {
            return;
        }

        let Some(item) = queue::dequeue_next() else {
            finish_or_continue(app);
            return;
        };

        CTRL.active.fetch_add(1, Ordering::SeqCst);
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            match item.kind {
                QueueKind::Crawl => crawl_url(&app2, item).await,
                QueueKind::Probe => probe_discovered_link(&app2, item).await,
            }
            CTRL.active.fetch_sub(1, Ordering::SeqCst);
            send_progress(&app2, "В процесі...", None);
            finish_or_continue(app2);
        });
    }
}

fn finish_or_continue(app: AppHandle) {
    if CTRL.finished.load(Ordering::SeqCst) {
        return;
    }
    let stopped = CTRL.stopped.load(Ordering::SeqCst);
    let paused = CTRL.paused.load(Ordering::SeqCst);
    let active = CTRL.active.load(Ordering::SeqCst);
    let pending = queue::has_pending_work();

    if stopped && active == 0 {
        complete_scan(&app, "Сканування зупинено.");
        return;
    }
    if paused {
        if active == 0 {
            send_progress(&app, "На паузі", None);
        }
        return;
    }
    if active < CTRL.concurrency.load(Ordering::SeqCst) && pending && !stopped {
        schedule_pump(app);
        return;
    }
    if active == 0 && !pending {
        let max = runtime().max_pages();
        let visited = runtime().visited_count();
        let msg = if max > 0 && visited >= max {
            format!("Досягнуто ліміт сторінок ({max}).")
        } else {
            "Сканування завершено.".to_string()
        };
        complete_scan(&app, &msg);
    }
}

fn complete_scan(app: &AppHandle, message: &str) {
    if CTRL
        .finished
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    emit::flush(app);
    emit::emit_referrers(app);
    send_progress(app, message, Some(true));
    emit::emit_end(app, message);
}

async fn crawl_url(app: &AppHandle, item: QueueItem) {
    if aborting() {
        return;
    }
    let rt = runtime();
    let config = rt.config();
    let url = item.url;

    if !rt.try_claim_url(&url) {
        return;
    }

    let robots = network::check_robots(&url, &config.user_agent, config.respect_robots).await;
    referrers::set_robots_fields(&url, robots.clone());
    if robots.robots_allowed == Some(false) && config.respect_robots {
        let mut result = results::build_spider_result(&url, &config.hostname);
        result.status = json!(0);
        result.fetched = false;
        results::apply_indexing_fields(&mut result, "", "", &robots);
        result.referrers = referrers::get_list(&url);
        rt.bump_scanned();
        emit::queue_result(app, result);
        return;
    }

    let mut tracker = RedirectTracker::new(url.clone());
    let mut current = url.clone();
    let mut first_status: Option<u16> = None;
    let mut first_redirect_target = String::new();
    let mut final_response = None;

    loop {
        if aborting() {
            return;
        }
        network::wait_before_request(config.request_delay_ms).await;
        let outcome = client()
            .fetch_page(&current, &config.user_agent, &config.auth, true)
            .await;

        let response = match outcome {
            Ok(r) => r,
            Err(err) => {
                let result = results::build_error_result(&url, &config.hostname, &err);
                rt.bump_scanned();
                emit::queue_result(app, result);
                return;
            }
        };

        if first_status.is_none() {
            first_status = Some(response.status);
        }

        if is_redirect_status(response.status) {
            let Some(loc) = response.location.as_deref() else {
                final_response = Some(response);
                break;
            };
            let Some(next) = resolve_redirect_target(&current, loc) else {
                final_response = Some(response);
                break;
            };
            if first_redirect_target.is_empty() {
                first_redirect_target = next.clone();
            }
            if !is_same_host(&next, &config.hostname) {
                tracker.record_hop(next.clone());
                let mut result = results::build_spider_result(&url, &config.hostname);
                result.status = json!(first_status.unwrap_or(response.status));
                result.fetched = true;
                result.content_type = response.content_type.clone();
                result.response_headers = response.headers.clone();
                result.response_time_ms = Some(response.elapsed_ms);
                results::apply_indexing_fields(&mut result, "", &response.x_robots_tag, &robots);
                tracker.to_fields(&mut result, &first_redirect_target);
                result.referrers = referrers::get_list(&url);
                rt.bump_scanned();
                emit::queue_result(app, result);
                return;
            }
            if !tracker.can_follow() || rt.is_visited(&next) || tracker.already_visited(&next) {
                tracker.mark_infinite(&next);
                let mut result = results::build_spider_result(&url, &config.hostname);
                result.status = json!(first_status.unwrap_or(response.status));
                result.fetched = true;
                tracker.to_fields(&mut result, &first_redirect_target);
                result.referrers = referrers::get_list(&url);
                rt.bump_scanned();
                emit::queue_result(app, result);
                return;
            }
            if !tracker.record_hop(next.clone()) {
                let mut result = results::build_spider_result(&url, &config.hostname);
                result.status = json!(first_status.unwrap_or(response.status));
                result.fetched = true;
                tracker.to_fields(&mut result, &first_redirect_target);
                result.referrers = referrers::get_list(&url);
                rt.bump_scanned();
                emit::queue_result(app, result);
                return;
            }
            referrers::add_referrer(&next, &current, Default::default());
            let _ = rt.try_claim_url(&next);

            if current != url {
                let mut hop = results::build_spider_result(&current, &config.hostname);
                hop.status = json!(response.status);
                hop.fetched = true;
                hop.redirect_hop_only = true;
                hop.redirect_url = next.clone();
                emit::queue_result(app, hop);
            }

            let hop_robots =
                network::check_robots(&next, &config.user_agent, config.respect_robots).await;
            if hop_robots.robots_allowed == Some(false) && config.respect_robots {
                let mut result = results::build_spider_result(&url, &config.hostname);
                result.status = json!(first_status.unwrap_or(response.status));
                result.fetched = true;
                tracker.to_fields(&mut result, &first_redirect_target);
                results::apply_indexing_fields(&mut result, "", "", &hop_robots);
                result.referrers = referrers::get_list(&url);
                rt.bump_scanned();
                emit::queue_result(app, result);
                return;
            }

            current = next;
            continue;
        }

        final_response = Some(response);
        break;
    }

    let Some(response) = final_response else {
        return;
    };

    let status_value = if tracker.hop_count() > 0 {
        json!(first_status.unwrap_or(response.status))
    } else {
        json!(response.status)
    };

    let mut result = results::build_spider_result(&url, &config.hostname);
    result.status = status_value;
    result.fetched = true;
    result.content_type = response.content_type.clone();
    result.response_headers = response.headers.clone();
    result.response_time_ms = Some(response.elapsed_ms);
    tracker.to_fields(&mut result, &first_redirect_target);

    if !(200..300).contains(&response.status) || !is_html_content(&response.content_type) {
        results::apply_indexing_fields(&mut result, "", &response.x_robots_tag, &robots);
        result.referrers = referrers::get_list(&url);
        rt.bump_scanned();
        emit::queue_result(app, result);
        return;
    }

    let body = response.body.clone().unwrap_or_default();
    let hostname = config.hostname.clone();
    let final_url = current.clone();
    let parsed = tokio::task::spawn_blocking(move || {
        parse_html_document(&body, &final_url, &hostname)
    })
    .await
    .unwrap_or_default();

    if aborting() {
        return;
    }

    result.title = parsed.title;
    result.meta_description = parsed.meta_description;
    result.meta_canonical = parsed.meta_canonical;
    result.headings = parsed.headings;
    result.og_title = parsed.og_title;
    result.og_description = parsed.og_description;
    result.og_image = parsed.og_image;
    result.kind = "html".into();
    results::apply_indexing_fields(
        &mut result,
        &parsed.meta_robots_raw,
        &response.x_robots_tag,
        &robots,
    );
    result.referrers = referrers::get_list(&url);
    let follow = result.meta_robots_status != "nofollow"
        && result.x_robots_tag_status != "nofollow"
        && result.meta_robots_status != "closed"
        && result.x_robots_tag_status != "closed";
    rt.bump_scanned();
    emit::queue_result(app, result);

    // Electron blocks follow on nofollow/closed; still emit stubs/probes.
    if follow {
        report_discovered_links(app, &url, &parsed.links).await;
    } else {
        let filtered: Vec<_> = parsed
            .links
            .into_iter()
            .map(|mut link| {
                if link.crawlable && !link.external {
                    link.crawlable = false;
                }
                link
            })
            .collect();
        report_discovered_links(app, &url, &filtered).await;
    }
}
