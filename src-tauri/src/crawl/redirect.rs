//! Redirect chain bookkeeping shared by the crawler and the link prober.

use crate::crawl::types::SpiderResult;
use std::collections::HashSet;

pub const MAX_REDIRECT_HOPS: u32 = 20;

/// Tracks the hops taken from one start URL and detects loops.
#[derive(Debug, Clone)]
pub struct RedirectTracker {
    start_url: String,
    chain: Vec<String>,
    seen: HashSet<String>,
    infinite: bool,
    loop_start_url: String,
}

impl RedirectTracker {
    pub fn new(start_url: impl Into<String>) -> Self {
        let start_url = start_url.into();
        let mut seen = HashSet::new();
        seen.insert(start_url.clone());
        Self {
            chain: vec![start_url.clone()],
            start_url,
            seen,
            infinite: false,
            loop_start_url: String::new(),
        }
    }

    pub fn start_url(&self) -> &str {
        &self.start_url
    }

    pub fn hop_count(&self) -> u32 {
        self.chain.len().saturating_sub(1) as u32
    }

    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    pub fn final_url(&self) -> &str {
        self.chain.last().map(String::as_str).unwrap_or(&self.start_url)
    }

    pub fn is_infinite(&self) -> bool {
        self.infinite
    }

    /// `false` once the hop budget is exhausted.
    pub fn can_follow(&self) -> bool {
        self.hop_count() < MAX_REDIRECT_HOPS
    }

    pub fn already_visited(&self, url: &str) -> bool {
        self.seen.contains(url)
    }

    /// Append a hop; returns `false` when the target closes a loop.
    pub fn record_hop(&mut self, url: impl Into<String>) -> bool {
        let url = url.into();
        if self.seen.contains(&url) {
            self.mark_infinite(&url);
            return false;
        }
        self.seen.insert(url.clone());
        self.chain.push(url);
        true
    }

    pub fn mark_infinite(&mut self, loop_start_url: &str) {
        self.infinite = true;
        if self.loop_start_url.is_empty() {
            self.loop_start_url = loop_start_url.to_string();
        }
    }

    /// Copy the `redirect*` fields onto a result row.
    pub fn to_fields(&self, result: &mut SpiderResult, redirect_url: &str) {
        result.redirect_hop_count = self.hop_count();
        result.redirect_infinite = self.infinite;
        result.redirect_loop_start_url = self.loop_start_url.clone();
        result.redirect_url = redirect_url.to_string();
        if self.hop_count() > 0 {
            result.redirect_final_url = self.final_url().to_string();
            result.redirect_chain = self.chain.clone();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawl::types::SpiderResult;

    #[test]
    fn detects_loop() {
        let mut t = RedirectTracker::new("https://a/");
        assert!(t.record_hop("https://b/"));
        assert!(!t.record_hop("https://a/"));
        let mut result = SpiderResult::default();
        t.to_fields(&mut result, "https://b/");
        assert!(result.redirect_infinite);
        assert_eq!(result.redirect_hop_count, 1);
    }
}
