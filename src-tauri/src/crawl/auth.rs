//! Optional HTTP authentication applied to same-host requests only.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use crate::crawl::url_utils::is_same_host;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthKind {
    None,
    Basic,
    Bearer,
}

impl AuthKind {
    pub fn from_str_lenient(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "basic" => AuthKind::Basic,
            "bearer" | "token" => AuthKind::Bearer,
            _ => AuthKind::None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub kind: AuthKind,
    pub username: String,
    pub password: String,
    pub token: String,
    /// Credentials are never leaked to third-party hosts.
    pub hostname: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            kind: AuthKind::None,
            username: String::new(),
            password: String::new(),
            token: String::new(),
            hostname: String::new(),
        }
    }
}

impl AuthConfig {
    pub fn new(
        auth_type: &str,
        username: &str,
        password: &str,
        token: &str,
        hostname: &str,
    ) -> Self {
        Self {
            kind: AuthKind::from_str_lenient(auth_type),
            username: username.to_string(),
            password: password.to_string(),
            token: token.to_string(),
            hostname: hostname.to_string(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.kind != AuthKind::None
    }

    /// `Authorization` header value for `url`, or `None` when it must be omitted.
    pub fn get_auth_header(&self, url: &str) -> Option<String> {
        if !self.is_enabled() || !is_same_host(url, &self.hostname) {
            return None;
        }
        match self.kind {
            AuthKind::None => None,
            AuthKind::Basic => {
                if self.username.is_empty() && self.password.is_empty() {
                    return None;
                }
                let raw = format!("{}:{}", self.username, self.password);
                Some(format!("Basic {}", BASE64.encode(raw.as_bytes())))
            }
            AuthKind::Bearer => {
                let token = self.token.trim();
                if token.is_empty() {
                    return None;
                }
                if token.to_ascii_lowercase().starts_with("bearer ") {
                    Some(token.to_string())
                } else {
                    Some(format!("Bearer {token}"))
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_same_host_only() {
        let auth = AuthConfig::new("basic", "u", "p", "", "ex.com");
        assert!(auth.get_auth_header("https://ex.com/a").unwrap().starts_with("Basic "));
        assert!(auth.get_auth_header("https://other.com/a").is_none());
    }
}
