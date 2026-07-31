//! HTML extraction with `scraper`: page metadata plus every outbound link.

use scraper::{ElementRef, Html, Selector};

use crate::crawl::types::{Heading, LinkMeta};
use crate::crawl::url_utils::{
    is_same_host, is_skippable_href, kind_from_url, parse_srcset_urls, resolve_url,
};

/// One discovered link edge.
#[derive(Debug, Clone)]
pub struct Link {
    pub url: String,
    pub text: String,
    pub external: bool,
    pub kind: String,
    pub tag: String,
    pub rel: String,
    pub rel_follow_allowed: Option<bool>,
    pub rel_index_allowed: Option<bool>,
    pub rel_label: String,
    pub img_alt_missing: bool,
    pub img_alt: Option<String>,
    /// Internal navigational link that should be fetched and parsed.
    pub crawlable: bool,
}

impl Link {
    pub fn to_meta(&self) -> LinkMeta {
        LinkMeta {
            text: self.text.clone(),
            rel: self.rel.clone(),
            tag: self.tag.clone(),
            kind: self.kind.clone(),
            rel_follow_allowed: self.rel_follow_allowed,
            rel_index_allowed: self.rel_index_allowed,
            rel_label: self.rel_label.clone(),
            img_alt_missing: self.img_alt_missing,
            img_alt: self.img_alt.clone(),
        }
    }
}

/// Everything extracted from one HTML document.
#[derive(Debug, Clone, Default)]
pub struct ParsedPage {
    pub title: String,
    pub meta_description: String,
    pub meta_canonical: String,
    pub meta_robots_raw: String,
    pub og_title: String,
    pub og_description: String,
    pub og_image: String,
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
}

fn select<'a>(document: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    match Selector::parse(selector) {
        Ok(parsed) => document.select(&parsed).collect(),
        Err(_) => Vec::new(),
    }
}

/// Collapse all whitespace runs into single spaces.
fn normalize_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn element_text(element: &ElementRef<'_>) -> String {
    normalize_text(&element.text().collect::<String>())
}

fn attr(element: &ElementRef<'_>, name: &str) -> Option<String> {
    element.value().attr(name).map(|value| value.to_string())
}

/// Split a `rel` attribute into follow/index verdicts, mirroring `parseLinkRel`.
pub fn parse_link_rel(rel: &str) -> (String, Option<bool>, Option<bool>, String) {
    let raw = rel.trim();
    if raw.is_empty() {
        return (String::new(), Some(true), Some(true), "follow".to_string());
    }
    let tokens: Vec<String> = raw
        .to_ascii_lowercase()
        .split([' ', ',', '\t', '\n'])
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect();

    let has_nofollow = tokens.iter().any(|t| t == "nofollow");
    let has_sponsored = tokens.iter().any(|t| t == "sponsored");
    let has_ugc = tokens.iter().any(|t| t == "ugc");
    let restricted = has_nofollow || has_sponsored || has_ugc;

    let markers: Vec<&str> = [
        if has_nofollow { "nofollow" } else { "" },
        if has_sponsored { "sponsored" } else { "" },
        if has_ugc { "ugc" } else { "" },
    ]
    .into_iter()
    .filter(|marker| !marker.is_empty())
    .collect();

    let label = if markers.is_empty() {
        raw.to_string()
    } else {
        markers.join(", ")
    };
    (
        raw.to_string(),
        Some(!restricted),
        Some(!restricted),
        label,
    )
}

/// Kind implied purely by the tag a link was found on.
fn kind_from_tag(tag: &str) -> Option<&'static str> {
    if tag.starts_with("script") || tag.contains("modulepreload") {
        return Some("javascript");
    }
    if tag.contains("stylesheet") {
        return Some("css");
    }
    if tag == "img[src]"
        || tag == "img[srcset]"
        || tag == "input[type=image][src]"
        || tag.contains("icon")
    {
        return Some("images");
    }
    if tag == "video[src]" || tag == "audio[src]" || tag == "source[src]" || tag == "source[srcset]"
    {
        return Some("media");
    }
    if tag == "embed[src]" || tag == "object[data]" {
        return Some("plugins");
    }
    if tag.contains("preconnect") || tag.contains("dns-prefetch") {
        return Some("other");
    }
    None
}

/// Navigational tags are classified by URL so a `.pdf` anchor stays a PDF.
fn is_navigational_tag(tag: &str) -> bool {
    matches!(tag, "a[href]" | "area[href]" | "form[action]" | "iframe[src]")
}

fn classify_kind(tag: &str, url: &str) -> String {
    if is_navigational_tag(tag) {
        let from_url = kind_from_url(url);
        if from_url == "other" {
            return "html".to_string();
        }
        return from_url;
    }
    if let Some(kind) = kind_from_tag(tag) {
        return kind.to_string();
    }
    kind_from_url(url)
}

fn is_crawlable(tag: &str, kind: &str, external: bool) -> bool {
    if external {
        return false;
    }
    match tag {
        "a[href]" | "area[href]" | "form[action]" => true,
        "iframe[src]" => kind == "html",
        _ => false,
    }
}

/// Parse a document and return its metadata plus the links it points at.
///
/// `allowed_hostname` decides which links count as internal.
pub fn parse_html_document(html: &str, current_url: &str, allowed_hostname: &str) -> ParsedPage {
    let document = Html::parse_document(html);
    let mut page = ParsedPage::default();

    if let Some(title) = select(&document, "title").first() {
        page.title = element_text(title);
    }

    for meta in select(&document, "meta") {
        let content = attr(&meta, "content").unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        let name = attr(&meta, "name").unwrap_or_default().to_ascii_lowercase();
        let property = attr(&meta, "property")
            .unwrap_or_default()
            .to_ascii_lowercase();

        match name.as_str() {
            "description" if page.meta_description.is_empty() => {
                page.meta_description = normalize_text(&content);
            }
            "robots" if page.meta_robots_raw.is_empty() => {
                page.meta_robots_raw = normalize_text(&content);
            }
            _ => {}
        }
        match property.as_str() {
            "og:title" if page.og_title.is_empty() => page.og_title = normalize_text(&content),
            "og:description" if page.og_description.is_empty() => {
                page.og_description = normalize_text(&content)
            }
            "og:image" if page.og_image.is_empty() => {
                page.og_image =
                    resolve_url(current_url, content.trim()).unwrap_or_else(|| content.clone());
            }
            "og:description" | "og:title" | "og:image" => {}
            _ => {}
        }
        if page.og_description.is_empty() && name == "og:description" {
            page.og_description = normalize_text(&content);
        }
    }

    for level in 1u8..=6 {
        for heading in select(&document, &format!("h{level}")) {
            let text = element_text(&heading);
            if text.is_empty() {
                continue;
            }
            page.headings.push(Heading { level, text });
        }
    }

    let mut collector = LinkCollector::new(current_url, allowed_hostname);

    for element in select(&document, "a[href]") {
        let href = attr(&element, "href").unwrap_or_default();
        let rel = attr(&element, "rel").unwrap_or_default();
        let text = element_text(&element);
        collector.push(&href, "a[href]", &text, Some(&rel), None);
    }

    for element in select(&document, "area[href]") {
        let href = attr(&element, "href").unwrap_or_default();
        let rel = attr(&element, "rel").unwrap_or_default();
        let text = attr(&element, "alt").unwrap_or_else(|| "area".to_string());
        collector.push(&href, "area[href]", &text, Some(&rel), None);
    }

    for element in select(&document, "link[href]") {
        let href = attr(&element, "href").unwrap_or_default();
        let rel_raw = attr(&element, "rel").unwrap_or_default();
        let rel_first = rel_raw
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if rel_first == "preconnect" || rel_first == "dns-prefetch" {
            continue;
        }
        let tag = if rel_first.is_empty() {
            "link[href]".to_string()
        } else {
            format!("link[rel={rel_first}]")
        };
        let text = if rel_raw.trim().is_empty() {
            "link".to_string()
        } else {
            format!("link {}", rel_raw.trim())
        };
        collector.push(&href, &tag, &text, None, None);
    }

    for element in select(&document, "script[src]") {
        let src = attr(&element, "src").unwrap_or_default();
        collector.push(&src, "script[src]", "script", None, None);
    }

    for element in select(&document, "iframe[src]") {
        let src = attr(&element, "src").unwrap_or_default();
        let text = attr(&element, "title").unwrap_or_else(|| "iframe".to_string());
        collector.push(&src, "iframe[src]", &text, None, None);
    }

    for element in select(&document, "embed[src]") {
        let src = attr(&element, "src").unwrap_or_default();
        collector.push(&src, "embed[src]", "embed", None, None);
    }

    for element in select(&document, "object[data]") {
        let data = attr(&element, "data").unwrap_or_default();
        collector.push(&data, "object[data]", "object", None, None);
    }

    for element in select(&document, "form[action]") {
        let action = attr(&element, "action").unwrap_or_default();
        collector.push(&action, "form[action]", "form", None, None);
    }

    for element in select(&document, "img") {
        let alt = attr(&element, "alt");
        let alt_missing = alt.as_ref().map(|value| value.trim().is_empty()).unwrap_or(true);
        let text = alt
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "image".to_string());
        let img_state = Some(ImgAlt {
            missing: alt_missing,
            value: alt.clone(),
        });

        if let Some(src) = attr(&element, "src") {
            collector.push(&src, "img[src]", &text, None, img_state.clone());
        }
        if let Some(srcset) = attr(&element, "srcset") {
            for candidate in parse_srcset_urls(&srcset) {
                collector.push(&candidate, "img[srcset]", &text, None, img_state.clone());
            }
        }
    }

    for element in select(&document, "source") {
        if let Some(src) = attr(&element, "src") {
            collector.push(&src, "source[src]", "media", None, None);
        }
        if let Some(srcset) = attr(&element, "srcset") {
            for candidate in parse_srcset_urls(&srcset) {
                collector.push(&candidate, "source[srcset]", "media", None, None);
            }
        }
    }

    for element in select(&document, "video[src]") {
        let src = attr(&element, "src").unwrap_or_default();
        collector.push(&src, "video[src]", "video", None, None);
    }

    for element in select(&document, "audio[src]") {
        let src = attr(&element, "src").unwrap_or_default();
        collector.push(&src, "audio[src]", "audio", None, None);
    }

    for element in select(&document, "input[src]") {
        let input_type = attr(&element, "type")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if input_type != "image" {
            continue;
        }
        let src = attr(&element, "src").unwrap_or_default();
        let alt = attr(&element, "alt");
        let alt_missing = alt.as_ref().map(|value| value.trim().is_empty()).unwrap_or(true);
        collector.push(
            &src,
            "input[type=image][src]",
            "input",
            None,
            Some(ImgAlt {
                missing: alt_missing,
                value: alt,
            }),
        );
    }

    if let Some(canonical) = select(&document, "link[rel=canonical]").first() {
        if let Some(href) = attr(canonical, "href") {
            page.meta_canonical = resolve_url(current_url, href.trim()).unwrap_or(href);
        }
    }

    page.links = collector.finish();
    page
}

#[derive(Debug, Clone)]
struct ImgAlt {
    missing: bool,
    value: Option<String>,
}

struct LinkCollector {
    base_url: String,
    hostname: String,
    seen: std::collections::HashSet<(String, String)>,
    links: Vec<Link>,
}

impl LinkCollector {
    fn new(base_url: &str, hostname: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            hostname: hostname.to_string(),
            seen: std::collections::HashSet::new(),
            links: Vec::new(),
        }
    }

    fn push(
        &mut self,
        href: &str,
        tag: &str,
        text: &str,
        rel: Option<&str>,
        img_alt: Option<ImgAlt>,
    ) {
        if is_skippable_href(href) {
            return;
        }
        let Some(url) = resolve_url(&self.base_url, href.trim()) else {
            return;
        };
        let key: (String, String) = (url.clone(), tag.to_string());
        if !self.seen.insert(key) {
            return;
        }

        let external = !is_same_host(&url, &self.hostname);
        let kind = classify_kind(tag, &url);
        let crawlable = is_crawlable(tag, &kind, external);

        let (rel_value, rel_follow, rel_index, rel_label) = match rel {
            Some(raw) => {
                let parsed = parse_link_rel(raw);
                (parsed.0, parsed.1, parsed.2, parsed.3)
            }
            None => (String::new(), None, None, String::new()),
        };

        self.links.push(Link {
            url,
            text: text.chars().take(300).collect(),
            external,
            kind,
            tag: tag.to_string(),
            rel: rel_value,
            rel_follow_allowed: rel_follow,
            rel_index_allowed: rel_index,
            rel_label,
            img_alt_missing: img_alt.as_ref().map(|alt| alt.missing).unwrap_or(false),
            img_alt: img_alt.and_then(|alt| alt.value),
            crawlable,
        });
    }

    fn finish(self) -> Vec<Link> {
        self.links
    }
}
