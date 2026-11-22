//! Desktop code that is the same on every OS: the log, recognising websites, and fetching and
//! caching icons. Each OS module (`win.rs`, `mac.rs`) re-exports this, so callers use `os::...`.

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;
static START: OnceLock<Instant> = OnceLock::new();

/// The log file: %APPDATA%oxa-nextoxa-desktop.log (the working directory is System32 when
/// Windows starts Voxa at sign-in). VOXA_LOG overrides it for tests.
pub fn log_path() -> std::path::PathBuf {
    match std::env::var_os("VOXA_LOG") {
        Some(p) => p.into(),
        None => crate::data_dir().join("voxa-desktop.log"),
    }
}

/// Append a line to the log and stdout. Never logs secrets: callers pass states and sizes only.
pub fn log(msg: &str) {
    let t = START.get_or_init(Instant::now).elapsed().as_millis();
    let line = format!("[{t:>7}ms] {msg}");
    println!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{line}");
    }
}
pub(crate) const SITES: &[(&str, &str, &str)] = &[
    ("chatgpt", "ChatGPT", "chatgpt.com"), ("gmail", "Gmail", "mail.google.com"), ("whatsapp", "WhatsApp", "whatsapp.com"),
    ("slack", "Slack", "slack.com"), ("claude", "Claude", "claude.ai"), ("google docs", "Google Docs", "docs.google.com"),
    ("google sheets", "Google Sheets", "sheets.google.com"), ("google meet", "Google Meet", "meet.google.com"),
    ("notion", "Notion", "notion.so"), ("github", "GitHub", "github.com"), ("linkedin", "LinkedIn", "linkedin.com"),
    ("youtube", "YouTube", "youtube.com"), ("outlook", "Outlook", "outlook.live.com"), ("discord", "Discord", "discord.com"),
    ("telegram", "Telegram", "telegram.org"), ("messenger", "Messenger", "messenger.com"), ("instagram", "Instagram", "instagram.com"),
    ("reddit", "Reddit", "reddit.com"), ("figma", "Figma", "figma.com"), ("jira", "Jira", "atlassian.com"),
    ("microsoft teams", "Teams", "teams.microsoft.com"), ("gemini", "Gemini", "gemini.google.com"),
    ("perplexity", "Perplexity", "perplexity.ai"), (" / x", "X", "x.com"),
];

pub fn site_from_title(title: &str) -> Option<String> {
    let title = title.to_lowercase();
    SITES.iter().find(|(k, _, _)| title.contains(k)).map(|(_, n, _)| n.to_string())
}

/// Fetch a known site's icon once, straight from the site (see `fetch_site_icon`).
pub fn cache_site_icon(site: &str) {
    if let Some(domain) = SITES.iter().find(|(_, n, _)| *n == site).map(|(_, _, d)| *d) {
        fetch_site_icon(domain.to_string(), icon_file(site));
    }
}

/// What History calls a domain: a known site's name ("Gmail" for mail.google.com), else the domain.
pub fn site_for_domain(domain: &str) -> String {
    SITES
        .iter()
        .find(|(_, _, d)| domain == *d || domain.ends_with(&format!(".{d}")))
        .map(|(_, n, _)| n.to_string())
        .unwrap_or_else(|| domain.to_string())
}

/// Icon for a name that is a service rather than an exe ("Claude Code" uses claude.ai's).
pub fn cache_named_icon(name: &str, domain: &str) {
    fetch_site_icon(domain.to_string(), icon_file(name));
}

/// The owning process id of a window, and its exe name without extension, lowercased.
pub fn cache_domain_icon(domain: &str) {
    fetch_site_icon(domain.to_string(), icon_file(domain));
}

/// Fetch a site's icon once, only from the site itself (no icon service, so nobody else learns
/// which sites are used): /apple-touch-icon.png, /favicon.ico, then the icon its home page
/// declares. Saved as a 64 px PNG; never fetched again. If none works, the letter badge stays.
pub(crate) fn fetch_site_icon(domain: String, out: std::path::PathBuf) {
    if out.exists() {
        return;
    }
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
        else {
            return;
        };
        let get = |url: &str| client.get(url).send().ok().filter(|r| r.status().is_success()).and_then(|r| r.bytes().ok());
        let decode = |b: &[u8]| image::load_from_memory(b).ok().filter(|i| i.width() >= 16);
        let mut found = None;
        for path in ["apple-touch-icon.png", "favicon.ico"] {
            if let Some(i) = get(&format!("https://{domain}/{path}")).and_then(|b| decode(&b)) {
                found = Some(i);
                break;
            }
        }
        if found.is_none() {
            let html = get(&format!("https://{domain}/")).map(|b| String::from_utf8_lossy(&b[..b.len().min(400_000)]).into_owned());
            for href in html.as_deref().map(icon_links).unwrap_or_default() {
                let url = resolve(&domain, &href);
                if let Some(i) = get(&url).and_then(|b| decode(&b)) {
                    found = Some(i);
                    break;
                }
            }
        }
        match found {
            Some(img) => {
                let img = img.resize(64, 64, image::imageops::FilterType::Lanczos3);
                // Centre on a square canvas so wide icons keep their shape.
                let mut sq = image::RgbaImage::new(64, 64);
                image::imageops::overlay(&mut sq, &img.to_rgba8(), ((64 - img.width()) / 2) as i64, ((64 - img.height()) / 2) as i64);
                if write_png(&out, sq.as_raw(), 64).is_ok() {
                    log(&format!("site icon cached for {domain}"));
                }
            }
            None => log(&format!("no site icon for {domain}; keeping the letter badge")),
        }
    });
}

/// `href`s of <link rel="...icon..."> tags, biggest-looking first (apple-touch-icon, then icon).
fn icon_links(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out: Vec<(u8, String)> = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<link") {
        let start = at + i;
        let end = lower[start..].find('>').map(|e| start + e).unwrap_or(lower.len());
        let tag = &lower[start..end];
        let attr = |name: &str| -> Option<(usize, usize)> {
            let p = tag.find(&format!("{name}="))? + name.len() + 1;
            let q = tag[p..].chars().next()?;
            if q == '"' || q == '\'' {
                let close = tag[p + 1..].find(q)?;
                Some((start + p + 1, start + p + 1 + close))
            } else {
                let close = tag[p..].find(|c: char| c.is_whitespace()).unwrap_or(tag.len() - p);
                Some((start + p, start + p + close))
            }
        };
        if let (Some((rs, re)), Some((hs, he))) = (attr("rel"), attr("href")) {
            let rel = &lower[rs..re];
            if rel.contains("icon") && !lower[hs..he].ends_with(".svg") {
                let rank = if rel.contains("apple-touch") { 0 } else { 1 };
                out.push((rank, html[hs..he].replace("&amp;", "&")));
            }
        }
        at = end;
    }
    out.sort_by_key(|(r, _)| *r);
    out.into_iter().map(|(_, h)| h).collect()
}

fn resolve(domain: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        format!("https://{domain}/{}", href.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod icon_tests {
    use super::{icon_links, resolve};

    #[test]
    fn finds_declared_icons() {
        let html = r#"<head><link rel="stylesheet" href="a.css"><link rel="icon" href="/fav-32.png" sizes="32x32">
            <link href='https://cdn.example.com/touch.png' rel='apple-touch-icon'><link rel="icon" href="/logo.svg"></head>"#;
        assert_eq!(icon_links(html), vec!["https://cdn.example.com/touch.png".to_string(), "/fav-32.png".to_string()]);
        assert_eq!(resolve("example.com", "/fav-32.png"), "https://example.com/fav-32.png");
        assert_eq!(resolve("example.com", "//cdn.x.com/i.png"), "https://cdn.x.com/i.png");
    }
}

/// Cache the icon for a domain: a known site's icon, else the site's own favicon.
pub fn cache_domain_icon_or_site(domain: &str) {
    let name = site_for_domain(domain);
    if is_known_site(&name) { cache_site_icon(&name) } else { cache_domain_icon(domain) }
}

pub fn is_known_site(name: &str) -> bool {
    SITES.iter().any(|(_, n, _)| *n == name)
}

#[cfg(test)]
mod tests {
    use super::site_from_title;

    #[test]
    fn sites_from_real_browser_titles() {
        assert_eq!(site_from_title("ChatGPT - Brave").as_deref(), Some("ChatGPT"));
        assert_eq!(site_from_title("Inbox (12) - someone@gmail.com - Gmail - Brave").as_deref(), Some("Gmail"));
        assert_eq!(site_from_title("(3) WhatsApp - Google Chrome").as_deref(), Some("WhatsApp"));
        assert_eq!(site_from_title("Project plan - Google Docs - Profile 1 - Microsoft Edge").as_deref(), Some("Google Docs"));
        assert_eq!(site_from_title("Home / X - Brave").as_deref(), Some("X"));
        assert_eq!(site_from_title("Rust docs - Brave"), None, "unknown sites stay as the browser");
    }
}

/// The app whose own icon represents `hwnd`: the browser for a site, else the app itself.
pub static OPEN_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn icon_file(app: &str) -> std::path::PathBuf {
    let slug: String = app.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    crate::data_dir().join("icons").join(format!("{}.png", slug.trim_matches('-')))
}

/// Save the real icon of the app that owns `hwnd` (64 px PNG), once per app. Runs off the UI thread.
pub(crate) fn write_png(out: &std::path::Path, rgba: &[u8], n: u32) -> std::io::Result<()> {
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = out.with_extension("tmp");
    {
        let f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        let mut enc = png::Encoder::new(f, n, n);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(std::io::Error::other)?;
        w.write_image_data(rgba).map_err(std::io::Error::other)?;
    }
    std::fs::rename(tmp, out)
}

/// Cache icons for apps in `wanted` (names already in History) that have a window open now, so
/// rows saved before icons existed get theirs too.
#[cfg(test)]
mod icon_probe {
    /// Manual: VOXA_DATA=<temp dir> cargo test probe_site_icons -- --ignored --nocapture
    #[test]
    #[ignore]
    fn probe_site_icons() {
        for d in ["chatgpt.com", "mail.google.com", "whatsapp.com", "slack.com", "claude.ai", "github.com", "youtube.com", "discord.com", "linkedin.com", "gemini.google.com", "mangafire.to"] {
            super::fetch_site_icon(d.to_string(), super::icon_file(d));
        }
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
}

