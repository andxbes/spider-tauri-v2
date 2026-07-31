//! Process-wide crawl state: visited sets, queues, robots cache and config.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};

use crate::crawl::auth::AuthConfig;
use crate::crawl::queue::QueueItem;
use crate::crawl::robots::RobotsTxt;
use crate::crawl::user_agent::DEFAULT_USER_AGENT;

/// Immutable per-scan configuration, swapped wholesale when a scan starts.
#[derive(Debug, Clone)]
pub struct SpiderConfig {
    pub start_url: String,
    pub hostname: String,
    pub origin: String,
    pub user_agent: String,
    pub auth: AuthConfig,
    pub request_delay_ms: u64,
    pub concurrency: usize,
    pub max_pages: usize,
    pub respect_robots: bool,
}

impl Default for SpiderConfig {
    fn default() -> Self {
        Self {
            start_url: String::new(),
            hostname: String::new(),
            origin: String::new(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            auth: AuthConfig::default(),
            request_delay_ms: 500,
            concurrency: 3,
            max_pages: 0,
            respect_robots: true,
        }
    }
}

pub struct CrawlRuntime {
    /// URLs claimed for crawling (one worker per URL, ever).
    pub visited: Mutex<HashSet<String>>,
    /// URLs already emitted as `fetched: false` stubs.
    pub reported_stubs: Mutex<HashSet<String>>,
    /// URLs already probed for a status code.
    pub probed: Mutex<HashSet<String>>,
    /// Dedupe set covering both queues.
    pub queued: Mutex<HashSet<String>>,
    pub crawl_queue: Mutex<VecDeque<QueueItem>>,
    pub probe_queue: Mutex<VecDeque<QueueItem>>,
    /// `None` marks an origin whose robots.txt could not be fetched.
    pub robots_cache: Mutex<HashMap<String, Option<Arc<RobotsTxt>>>>,
    pub config: RwLock<Arc<SpiderConfig>>,
    pub max_pages: AtomicUsize,
    pub respect_robots: AtomicBool,
    /// Number of rows reported with `fetched: true`.
    pub scanned: AtomicUsize,
}

static RUNTIME: Lazy<CrawlRuntime> = Lazy::new(CrawlRuntime::new);

pub fn runtime() -> &'static CrawlRuntime {
    &RUNTIME
}

impl CrawlRuntime {
    fn new() -> Self {
        Self {
            visited: Mutex::new(HashSet::new()),
            reported_stubs: Mutex::new(HashSet::new()),
            probed: Mutex::new(HashSet::new()),
            queued: Mutex::new(HashSet::new()),
            crawl_queue: Mutex::new(VecDeque::new()),
            probe_queue: Mutex::new(VecDeque::new()),
            robots_cache: Mutex::new(HashMap::new()),
            config: RwLock::new(Arc::new(SpiderConfig::default())),
            max_pages: AtomicUsize::new(0),
            respect_robots: AtomicBool::new(true),
            scanned: AtomicUsize::new(0),
        }
    }

    pub fn config(&self) -> Arc<SpiderConfig> {
        self.config.read().clone()
    }

    pub fn set_config(&self, config: SpiderConfig) {
        self.max_pages.store(config.max_pages, Ordering::SeqCst);
        self.respect_robots
            .store(config.respect_robots, Ordering::SeqCst);
        *self.config.write() = Arc::new(config);
    }

    pub fn hostname(&self) -> String {
        self.config().hostname.clone()
    }

    pub fn respect_robots(&self) -> bool {
        self.respect_robots.load(Ordering::SeqCst)
    }

    pub fn max_pages(&self) -> usize {
        self.max_pages.load(Ordering::SeqCst)
    }

    /// `true` once the configured page budget is used up (0 means unlimited).
    pub fn page_limit_reached(&self) -> bool {
        let max = self.max_pages();
        max > 0 && self.visited.lock().len() >= max
    }

    /// Atomically reserve a URL for crawling. Returns `false` when it was
    /// already claimed or the page budget is exhausted.
    pub fn try_claim_url(&self, url: &str) -> bool {
        let max = self.max_pages();
        let mut visited = self.visited.lock();
        if max > 0 && visited.len() >= max && !visited.contains(url) {
            return false;
        }
        visited.insert(url.to_string())
    }

    pub fn is_visited(&self, url: &str) -> bool {
        self.visited.lock().contains(url)
    }

    /// Returns `true` the first time a URL is reported as a stub.
    pub fn mark_stub_reported(&self, url: &str) -> bool {
        self.reported_stubs.lock().insert(url.to_string())
    }

    /// Returns `true` the first time a URL is claimed for probing.
    pub fn mark_probed(&self, url: &str) -> bool {
        self.probed.lock().insert(url.to_string())
    }

    pub fn is_probed(&self, url: &str) -> bool {
        self.probed.lock().contains(url)
    }

    pub fn visited_count(&self) -> usize {
        self.visited.lock().len()
    }

    pub fn scanned_count(&self) -> usize {
        self.scanned.load(Ordering::SeqCst)
    }

    pub fn bump_scanned(&self) -> usize {
        self.scanned.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn clear(&self) {
        self.visited.lock().clear();
        self.reported_stubs.lock().clear();
        self.probed.lock().clear();
        self.queued.lock().clear();
        self.crawl_queue.lock().clear();
        self.probe_queue.lock().clear();
        self.robots_cache.lock().clear();
        self.scanned.store(0, Ordering::SeqCst);
    }
}
