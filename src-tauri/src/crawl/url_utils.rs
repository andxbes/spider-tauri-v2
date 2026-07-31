use url::Url;

const MEDIA_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "svg", "ico", "bmp", "avif",
    "mp4", "webm", "mp3", "wav", "ogg", "avi", "mov", "m4a", "flac",
    "pdf", "zip", "rar", "7z", "gz", "tar", "css", "js", "mjs", "cjs", "map",
    "woff", "woff2", "ttf", "otf", "eot", "xml", "json", "txt", "csv",
];

pub fn normalize_page_url(raw: &str) -> Option<String> {
    let mut u = Url::parse(raw).ok()?;
    u.set_fragment(None);
    Some(u.to_string())
}

pub fn is_same_host(url: &str, hostname: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.eq_ignore_ascii_case(hostname)))
        .unwrap_or(false)
}

pub fn is_skippable_href(href: &str) -> bool {
    let h = href.trim().to_ascii_lowercase();
    h.is_empty()
        || h.starts_with('#')
        || h.starts_with("javascript:")
        || h.starts_with("mailto:")
        || h.starts_with("tel:")
        || h.starts_with("data:")
        || h.starts_with("blob:")
}

pub fn is_redirect_status(status: u16) -> bool {
    (300..400).contains(&status)
}

pub fn resolve_redirect_target(from: &str, location: &str) -> Option<String> {
    let base = Url::parse(from).ok()?;
    let joined = base.join(location.trim()).ok()?;
    normalize_page_url(joined.as_str())
}

pub fn get_content_type(ct: Option<&str>) -> String {
    ct.unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

pub fn is_html_content(ct: &str) -> bool {
    ct.is_empty() || ct.contains("text/html") || ct.contains("application/xhtml")
}

pub fn get_url_extension(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| {
            PathLike::ext(u.path())
        })
        .unwrap_or_default()
}

struct PathLike;
impl PathLike {
    fn ext(path: &str) -> Option<String> {
        let name = path.rsplit('/').next().unwrap_or("");
        let (_, ext) = name.rsplit_once('.')?;
        if ext.is_empty() || ext.len() > 8 { return None; }
        Some(ext.to_ascii_lowercase())
    }
}

pub fn is_likely_media_url(url: &str) -> bool {
    let ext = get_url_extension(url);
    MEDIA_EXTS.iter().any(|e| *e == ext)
}

pub fn parse_srcset_urls(srcset: &str) -> Vec<String> {
    srcset
        .split(',')
        .filter_map(|part| {
            let t = part.trim();
            if t.is_empty() { return None; }
            Some(t.split_whitespace().next()?.to_string())
        })
        .collect()
}

pub fn hostname_of(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(|s| s.to_string())
}

pub fn origin_of(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    let host = u.host_str()?;
    // `port()` is None for the scheme's default port, so it never leaks :80/:443.
    Some(match u.port() {
        Some(port) => format!("{}://{}:{}", u.scheme(), host, port),
        None => format!("{}://{}", u.scheme(), host),
    })
}

pub fn get_origin(url: &str) -> String {
    origin_of(url).unwrap_or_default()
}

pub fn resolve_url(base: &str, href: &str) -> Option<String> {
    let base = Url::parse(base).ok()?;
    let joined = base.join(href.trim()).ok()?;
    normalize_page_url(joined.as_str())
}

pub fn kind_from_url(url: &str) -> String {
    let ext = get_url_extension(url);
    match ext.as_str() {
        "js" | "mjs" | "cjs" => "javascript".into(),
        "css" => "css".into(),
        "woff" | "woff2" | "ttf" | "otf" | "eot" => "fonts".into(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "ico" | "bmp" | "avif" => "images".into(),
        "mp4" | "webm" | "mp3" | "wav" | "ogg" | "avi" | "mov" => "media".into(),
        "xml" => "xml".into(),
        "pdf" => "pdf".into(),
        "html" | "htm" | "xhtml" => "html".into(),
        _ => "other".into(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_hash() {
        assert_eq!(
            normalize_page_url("https://ex.com/a#x").unwrap(),
            "https://ex.com/a"
        );
    }

    #[test]
    fn same_host() {
        assert!(is_same_host("https://Ex.com/a", "ex.com"));
        assert!(!is_same_host("https://other.com/a", "ex.com"));
    }

    #[test]
    fn srcset() {
        let v = parse_srcset_urls("a.jpg 1x, b.jpg 2x");
        assert_eq!(v, vec!["a.jpg", "b.jpg"]);
    }

    #[test]
    fn origin_keeps_non_default_port() {
        assert_eq!(get_origin("http://ex.com:8080/a"), "http://ex.com:8080");
        assert_eq!(get_origin("https://ex.com/a"), "https://ex.com");
        assert_eq!(get_origin("http://ex.com:80/a"), "http://ex.com");
    }
}
