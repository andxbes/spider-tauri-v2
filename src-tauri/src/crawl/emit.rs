//! Event emission with result coalescing.
//!
//! Rows are buffered and flushed either every 150 ms or once 500 have piled
//! up, so a fast crawl does not drown the webview in IPC messages.

use std::time::Duration;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::crawl::referrers;
use crate::crawl::types::{ProgressPayload, SpiderResult};

pub const FLUSH_INTERVAL_MS: u64 = 150;
pub const FLUSH_BATCH_SIZE: usize = 500;

pub const EVENT_RESULT: &str = "spider-result";
pub const EVENT_RESULTS_BATCH: &str = "spider-results-batch";
pub const EVENT_PROGRESS: &str = "spider-progress";
pub const EVENT_REFERRERS: &str = "spider-referrers-update";
pub const EVENT_END: &str = "spider-end";

static BUFFER: Lazy<Mutex<Vec<SpiderResult>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Buffer a result row, flushing early when the batch is full.
pub fn queue_result(app: &AppHandle, result: SpiderResult) {
    let should_flush = {
        let mut buffer = BUFFER.lock();
        buffer.push(result);
        buffer.len() >= FLUSH_BATCH_SIZE
    };
    if should_flush {
        flush(app);
    }
}

/// Send everything buffered so far.
pub fn flush(app: &AppHandle) {
    let items = {
        let mut buffer = BUFFER.lock();
        if buffer.is_empty() {
            return;
        }
        std::mem::take(&mut *buffer)
    };
    if items.len() == 1 {
        let _ = app.emit(EVENT_RESULT, &items[0]);
    } else {
        let _ = app.emit(EVENT_RESULTS_BATCH, &items);
    }
}

pub fn clear_buffer() {
    BUFFER.lock().clear();
}

pub fn emit_progress(app: &AppHandle, payload: &ProgressPayload) {
    let _ = app.emit(EVENT_PROGRESS, payload);
}

/// Ship the inbound-link graph (or ask the renderer to rebuild it locally).
pub fn emit_referrers(app: &AppHandle) {
    let payload = referrers::build_all_payload();
    let _ = app.emit(EVENT_REFERRERS, &payload);
}

pub fn emit_end(app: &AppHandle, message: &str) {
    let _ = app.emit(EVENT_END, message);
}

/// Background flusher; exits as soon as `keep_running` reports `false`.
pub fn spawn_flusher<F>(app: AppHandle, keep_running: F)
where
    F: Fn() -> bool + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(FLUSH_INTERVAL_MS)).await;
            flush(&app);
            if !keep_running() {
                flush(&app);
                break;
            }
        }
    });
}
