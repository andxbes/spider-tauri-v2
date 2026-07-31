//! Minimal robots.txt parser: `User-agent` / `Allow` / `Disallow` / `Sitemap`.
//!
//! Longest-match wins, `Allow` beats `Disallow` on ties, and `*` / `$`
//! wildcards are supported — enough for crawl gating without a full REP crate.

#[derive(Debug, Clone)]
pub struct RobotsRule {
    pub allow: bool,
    pub pattern: String,
}

#[derive(Debug, Clone, Default)]
pub struct RobotsGroup {
    pub agents: Vec<String>,
    pub rules: Vec<RobotsRule>,
}

#[derive(Debug, Clone, Default)]
pub struct RobotsTxt {
    pub groups: Vec<RobotsGroup>,
    pub sitemaps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RobotsDecision {
    pub allowed: bool,
    pub rule: String,
}

impl Default for RobotsDecision {
    fn default() -> Self {
        Self {
            allowed: true,
            rule: String::new(),
        }
    }
}

impl RobotsTxt {
    pub fn parse(text: &str) -> Self {
        let mut groups: Vec<RobotsGroup> = Vec::new();
        let mut sitemaps: Vec<String> = Vec::new();
        let mut current: Option<RobotsGroup> = None;
        // A `User-agent` line right after a rule starts a fresh group.
        let mut last_line_was_rule = false;

        for raw_line in text.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((field, value)) = line.split_once(':') else {
                continue;
            };
            let field = field.trim().to_ascii_lowercase();
            let value = value.trim();

            match field.as_str() {
                "user-agent" => {
                    if last_line_was_rule {
                        if let Some(group) = current.take() {
                            groups.push(group);
                        }
                    }
                    last_line_was_rule = false;
                    let group = current.get_or_insert_with(RobotsGroup::default);
                    group.agents.push(value.to_ascii_lowercase());
                }
                "allow" | "disallow" => {
                    last_line_was_rule = true;
                    let group = current.get_or_insert_with(RobotsGroup::default);
                    if group.agents.is_empty() {
                        group.agents.push("*".to_string());
                    }
                    // `Disallow:` with an empty value means "allow everything".
                    if value.is_empty() && field == "disallow" {
                        continue;
                    }
                    group.rules.push(RobotsRule {
                        allow: field == "allow",
                        pattern: value.to_string(),
                    });
                }
                "sitemap" => {
                    if !value.is_empty() {
                        sitemaps.push(value.to_string());
                    }
                }
                _ => {}
            }
        }
        if let Some(group) = current.take() {
            groups.push(group);
        }

        Self { groups, sitemaps }
    }

    /// Pick the most specific matching group, falling back to `User-agent: *`.
    fn select_group(&self, user_agent: &str) -> Option<&RobotsGroup> {
        let ua_lower = user_agent.to_ascii_lowercase();
        let mut best: Option<(usize, &RobotsGroup)> = None;
        let mut wildcard: Option<&RobotsGroup> = None;

        for group in &self.groups {
            for agent in &group.agents {
                if agent == "*" {
                    if wildcard.is_none() {
                        wildcard = Some(group);
                    }
                    continue;
                }
                if agent.is_empty() || !ua_lower.contains(agent.as_str()) {
                    continue;
                }
                if best.map_or(true, |(len, _)| agent.len() > len) {
                    best = Some((agent.len(), group));
                }
            }
        }
        best.map(|(_, group)| group).or(wildcard)
    }

    /// Evaluate a path (with query string) for the given user agent.
    pub fn is_allowed(&self, path_with_query: &str, user_agent: &str) -> RobotsDecision {
        let Some(group) = self.select_group(user_agent) else {
            return RobotsDecision::default();
        };
        let mut best: Option<&RobotsRule> = None;
        for rule in &group.rules {
            if !pattern_matches(&rule.pattern, path_with_query) {
                continue;
            }
            best = match best {
                None => Some(rule),
                Some(current) => {
                    if rule.pattern.len() > current.pattern.len()
                        || (rule.pattern.len() == current.pattern.len() && rule.allow)
                    {
                        Some(rule)
                    } else {
                        Some(current)
                    }
                }
            };
        }
        match best {
            None => RobotsDecision::default(),
            Some(rule) => RobotsDecision {
                allowed: rule.allow,
                rule: format!(
                    "{}: {}",
                    if rule.allow { "Allow" } else { "Disallow" },
                    rule.pattern
                ),
            },
        }
    }
}

/// robots.txt glob matching: `*` matches any run of characters, a trailing `$`
/// anchors the pattern to the end of the path.
pub fn pattern_matches(pattern: &str, path: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let pattern = if anchored {
        &pattern[..pattern.len() - 1]
    } else {
        pattern
    };
    if pattern.is_empty() {
        return !anchored || path.is_empty();
    }

    let segments: Vec<&str> = pattern.split('*').collect();
    let last_index = segments.len() - 1;
    let mut cursor = 0usize;

    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }
        if cursor > path.len() {
            return false;
        }
        if index == 0 {
            if !path[cursor..].starts_with(segment) {
                return false;
            }
            cursor += segment.len();
            continue;
        }
        if index == last_index && anchored {
            if path.len() < cursor + segment.len() || !path.ends_with(segment) {
                return false;
            }
            cursor = path.len();
            continue;
        }
        match path[cursor..].find(segment) {
            Some(offset) => cursor += offset + segment.len(),
            None => return false,
        }
    }

    if anchored && segments[last_index].is_empty() {
        // Pattern ended with `*$`, which matches any tail.
        return true;
    }
    if anchored && cursor != path.len() {
        return false;
    }
    true
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disallow_admin() {
        let robots = RobotsTxt::parse("User-agent: *\nDisallow: /admin\n");
        let denied = robots.is_allowed("/admin/x", "MyTauriSpider/1.0");
        assert!(!denied.allowed);
        let allowed = robots.is_allowed("/public", "MyTauriSpider/1.0");
        assert!(allowed.allowed);
    }

    #[test]
    fn extracts_sitemaps() {
        let robots = RobotsTxt::parse("Sitemap: https://ex.com/sitemap.xml\n");
        assert_eq!(robots.sitemaps, vec!["https://ex.com/sitemap.xml"]);
    }
}
