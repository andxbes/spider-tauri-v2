//! HTTP layer: a shared reqwest client with manual redirect handling,
//! throttling with jitter, and a cached robots.txt fetcher.

use std::sync::Arc;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use rand::Rng;
use reqwest::header::{HeaderMap, ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, Response};
use url::Url;

use crate::crawl::auth::AuthConfig;
use crate::crawl::robots::RobotsTxt;
use crate::crawl::state::runtime;
use crate::crawl::types::RobotsFields;
use crate::crawl::url_utils::{get_content_type, get_origin, is_html_content};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
pub const SITEMAP_TIMEOUT: Duration = Duration::from_secs(60);
/// Cap on the HTML we keep in memory for a single page (8 MiB).
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// A response reduced to the fields the crawler cares about.
/// Full header maps are not retained — live UI / dumps omit them for RAM.
#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub url: String,
    pub status: u16,
    pub content_type: String,
    pub location: Option<String>,
    pub x_robots_tag: String,
    pub elapsed_ms: u64,
    pub body: Option<String>,
}

pub struct HttpClient {
    client: Client,
}

static CLIENT: Lazy<HttpClient> = Lazy::new(HttpClient::new);

pub fn client() -> &'static HttpClient {
    &CLIENT
}

impl HttpClient {
    fn new() -> Self {
        let client = Client::builder()
            // Redirects are followed by hand so every hop can be reported.
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(false)
            .pool_max_idle_per_host(50)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    fn build_headers(&self, url: &str, user_agent: &str, auth: &AuthConfig) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = user_agent.parse() {
            headers.insert(USER_AGENT, value);
        }
        if let Ok(value) = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".parse()
        {
            headers.insert(ACCEPT, value);
        }
        if let Ok(value) = "en-US,en;q=0.9".parse() {
            headers.insert(ACCEPT_LANGUAGE, value);
        }
        if let Some(header) = auth.get_auth_header(url) {
            if let Ok(value) = header.parse() {
                headers.insert(AUTHORIZATION, value);
            }
        }
        headers
    }

    /// Perform a single request without following redirects.
    pub async fn fetch(
        &self,
        url: &str,
        method: Method,
        user_agent: &str,
        auth: &AuthConfig,
        timeout: Duration,
        read_body: bool,
    ) -> Result<FetchResponse, String> {
        let started = Instant::now();
        let response: Response = self
            .client
            .request(method, url)
            .headers(self.build_headers(url, user_agent, auth))
            .timeout(timeout)
            .send()
            .await
            .map_err(format_request_error)?;

        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = get_content_type(
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
        );
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let x_robots_tag = response
            .headers()
            .get_all("x-robots-tag")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        let body = if read_body && is_html_content(&content_type) {
            match response.text().await {
                Ok(text) => {
                    if text.len() > MAX_BODY_BYTES {
                        Some(truncate_on_char_boundary(&text, MAX_BODY_BYTES).to_string())
                    } else {
                        Some(text)
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        };

        Ok(FetchResponse {
            url: final_url,
            status,
            content_type,
            location,
            x_robots_tag,
            elapsed_ms: started.elapsed().as_millis() as u64,
            body,
        })
    }

    pub async fn fetch_page(
        &self,
        url: &str,
        user_agent: &str,
        auth: &AuthConfig,
        read_body: bool,
    ) -> Result<FetchResponse, String> {
        self.fetch(
            url,
            Method::GET,
            user_agent,
            auth,
            DEFAULT_TIMEOUT,
            read_body,
        )
        .await
    }

    /// `HEAD` first, falling back to `GET` for servers that reject it.
    pub async fn probe(
        &self,
        url: &str,
        user_agent: &str,
        auth: &AuthConfig,
    ) -> Result<FetchResponse, String> {
        match self
            .fetch(url, Method::HEAD, user_agent, auth, DEFAULT_TIMEOUT, false)
            .await
        {
            Ok(response) if !head_rejected(response.status) => Ok(response),
            Ok(_) | Err(_) => {
                self.fetch(url, Method::GET, user_agent, auth, DEFAULT_TIMEOUT, false)
                    .await
            }
        }
    }

    /// Plain text fetch used for robots.txt and sitemaps.
    pub async fn fetch_text(
        &self,
        url: &str,
        user_agent: &str,
        auth: &AuthConfig,
        timeout: Duration,
    ) -> Result<(u16, String), String> {
        let response = self
            .client
            .get(url)
            .headers(self.build_headers(url, user_agent, auth))
            .timeout(timeout)
            .send()
            .await
            .map_err(format_request_error)?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        Ok((status, text))
    }
}

fn head_rejected(status: u16) -> bool {
    matches!(status, 400 | 403 | 405 | 501)
}

fn truncate_on_char_boundary(text: &str, max_bytes: usize) -> &str {
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn format_request_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        return "Timeout".to_string();
    }
    if error.is_connect() {
        return format!("Connection failed: {error}");
    }
    error.to_string()
}

/// Per-worker polite delay (+/-20% jitter). Independent across workers so
/// concurrency is real; delay paces each worker, not the whole pool.
pub async fn wait_before_request(delay_ms: u64) {
    if delay_ms == 0 {
        return;
    }
    let jitter: f64 = rand::thread_rng().gen_range(0.8..1.2);
    let wait = Duration::from_millis(((delay_ms as f64) * jitter).round() as u64);
    tokio::time::sleep(wait).await;
}

/// Fetch and cache the robots.txt for an origin. `None` means "unavailable",
/// which is treated as "everything allowed".
pub async fn get_robots(origin: &str, user_agent: &str) -> Option<Arc<RobotsTxt>> {
    if origin.is_empty() {
        return None;
    }
    if let Some(cached) = runtime().robots_cache.lock().get(origin) {
        return cached.clone();
    }

    let url = format!("{}/robots.txt", origin.trim_end_matches('/'));
    let parsed = match client()
        .fetch_text(&url, user_agent, &AuthConfig::default(), DEFAULT_TIMEOUT)
        .await
    {
        Ok((status, text)) if (200..300).contains(&status) && !text.is_empty() => {
            Some(Arc::new(RobotsTxt::parse(&text)))
        }
        _ => None,
    };

    runtime()
        .robots_cache
        .lock()
        .insert(origin.to_string(), parsed.clone());
    parsed
}

/// Robots.txt verdict for a URL, or an empty verdict when robots are ignored.
pub async fn check_robots(url: &str, user_agent: &str, respect: bool) -> RobotsFields {
    if !respect {
        return RobotsFields::default();
    }
    let origin = get_origin(url);
    let Some(robots) = get_robots(&origin, user_agent).await else {
        return RobotsFields::default();
    };
    let path = path_with_query(url);
    let decision = robots.is_allowed(&path, user_agent);
    RobotsFields {
        robots_allowed: Some(decision.allowed),
        robots_rule: decision.rule,
    }
}

pub fn path_with_query(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) => match parsed.query() {
            Some(query) => format!("{}?{}", parsed.path(), query),
            None => parsed.path().to_string(),
        },
        Err(_) => "/".to_string(),
    }
}

/// Sitemap URLs advertised by an origin's robots.txt.
pub async fn robots_sitemaps(origin: &str, user_agent: &str) -> Vec<String> {
    get_robots(origin, user_agent)
        .await
        .map(|robots| robots.sitemaps.clone())
        .unwrap_or_default()
}
