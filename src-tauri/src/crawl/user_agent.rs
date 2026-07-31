//! User-Agent presets, kept in sync with `src/shared/user-agents.js`.

pub const DEFAULT_USER_AGENT: &str = "MyTauriSpider/1.0";
pub const DEFAULT_USER_AGENT_PRESET_ID: &str = "spider";
pub const CUSTOM_USER_AGENT_PRESET_ID: &str = "custom";

pub const USER_AGENT_PRESETS: &[(&str, &str)] = &[
    ("spider", DEFAULT_USER_AGENT),
    (
        "chrome-win",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    ),
    (
        "chrome-mac",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7_1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    ),
    (
        "firefox-win",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    ),
    (
        "safari-mac",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7_1) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15",
    ),
    (
        "edge-win",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
    ),
    (
        "googlebot",
        "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    ),
    (
        "bingbot",
        "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
    ),
    (
        "yandexbot",
        "Mozilla/5.0 (compatible; YandexBot/3.0; +http://yandex.com/bots)",
    ),
    (
        "duckduckbot",
        "DuckDuckBot/1.0; (+http://duckduckgo.com/duckduckbot.html)",
    ),
    (CUSTOM_USER_AGENT_PRESET_ID, ""),
];

pub fn is_valid_preset_id(id: &str) -> bool {
    USER_AGENT_PRESETS.iter().any(|(key, _)| *key == id)
}

pub fn get_preset_value(id: &str) -> Option<&'static str> {
    USER_AGENT_PRESETS
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, value)| *value)
}

/// Resolve the effective UA string from a preset id plus a custom override.
pub fn resolve_user_agent(preset: &str, custom: &str) -> String {
    let preset = preset.trim();
    let custom = custom.trim();
    let preset = if is_valid_preset_id(preset) {
        preset
    } else {
        DEFAULT_USER_AGENT_PRESET_ID
    };
    if preset == CUSTOM_USER_AGENT_PRESET_ID {
        if custom.is_empty() {
            return DEFAULT_USER_AGENT.to_string();
        }
        return custom.to_string();
    }
    match get_preset_value(preset) {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => DEFAULT_USER_AGENT.to_string(),
    }
}

/// The token robots.txt groups are matched against (first `/`-free word, lowercased).
pub fn robots_token(user_agent: &str) -> String {
    user_agent
        .split(['/', ' '])
        .next()
        .unwrap_or(user_agent)
        .trim()
        .to_ascii_lowercase()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spider() {
        assert_eq!(resolve_user_agent("spider", ""), DEFAULT_USER_AGENT);
    }

    #[test]
    fn custom_ua() {
        assert_eq!(resolve_user_agent("custom", "Bot/1"), "Bot/1");
    }
}
