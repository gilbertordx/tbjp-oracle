//! Downloads forum replies and parses them into `RawPost`s.
//!
//! Python equivalent: `src/scraper.py` (`requests` + `BeautifulSoup`).
//! Rust equivalents: `reqwest` (HTTP) and `scraper` (HTML + CSS selectors).
//!
//! The parsing is split from the downloading on purpose: `parse_page` takes a
//! string and returns posts, so it can be unit-tested with no network.

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;
use chrono::Local;
use scraper::{ElementRef, Html, Node, Selector};

use crate::dates::parse_forum_date;
use crate::models::RawPost;

const BASE_URL: &str = "https://www.trainedbyjp.com/members/jordan-peters/forums/replies/";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

/// Find the value after `prefix` in a list of CSS classes, e.g. the class
/// `post-1496021` with prefix `post-` gives `1496021`.
fn id_from_classes<'a>(classes: impl Iterator<Item = &'a str>, prefix: &str) -> Option<String> {
    classes
        .into_iter()
        .find_map(|class| class.strip_prefix(prefix).map(str::to_string))
}

/// All text under `element`, each piece trimmed, empties dropped, joined with
/// newlines (BeautifulSoup: `get_text(strip=True, separator="\n")`) while
/// skipping any `.bbp-signature` subtree (Python: `signature.decompose()`).
fn clean_text(element: ElementRef) -> String {
    fn walk(element: ElementRef, out: &mut Vec<String>) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_string());
                    }
                }
                Node::Element(el) => {
                    if el.classes().any(|c| c == "bbp-signature") {
                        continue;
                    }
                    if let Some(child_el) = ElementRef::wrap(child) {
                        walk(child_el, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut pieces = Vec::new();
    walk(element, &mut pieces);
    pieces.join("\n")
}

/// Parse one page of the replies listing. `page_url` is only used in log lines.
pub fn parse_page(html: &str, page_url: &str) -> Vec<RawPost> {
    // `.unwrap()` is acceptable here because these selector strings are
    // constants that we know are valid; a typo would fail in the tests.
    let item_sel = Selector::parse("div.bs-reply-list-item").unwrap();
    let content_sel = Selector::parse("div.bbp-reply-content").unwrap();
    let title_sel = Selector::parse("a.bbp-topic-permalink").unwrap();
    let date_sel = Selector::parse("span.bs-timestamp").unwrap();

    let document = Html::parse_document(html);
    let mut posts = Vec::new();

    for (idx, item) in document.select(&item_sel).enumerate() {
        let idx = idx + 1;
        let post_id = id_from_classes(item.value().classes(), "post-");
        let thread_id = id_from_classes(item.value().classes(), "bbp-parent-topic-");
        let (Some(post_id), Some(thread_id)) = (post_id.clone(), thread_id.clone()) else {
            println!("SKIP missing IDs at {page_url} item {idx}: post_id={post_id:?}, thread_id={thread_id:?}");
            continue;
        };

        let Some(content_div) = item.select(&content_sel).next() else { continue };
        let content = clean_text(content_div);
        if content.is_empty() {
            continue;
        }

        let thread_title = item
            .select(&title_sel)
            .next()
            .map(|a| a.text().collect::<String>().trim().to_string());

        let date = item
            .select(&date_sel)
            .next()
            .map(|s| s.text().collect::<String>().trim().to_string());
        let timestamp = date
            .as_deref()
            .and_then(parse_forum_date)
            .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string());
        if let (Some(d), None) = (&date, &timestamp) {
            println!("WARN unparseable date at {page_url} item {idx}: {d}");
        }

        posts.push(RawPost {
            post_id,
            thread_id,
            thread_title,
            author: Some("Jordan Peters".to_string()),
            timestamp,
            date,
            content: Some(content),
            parent_post_id: None,
            tags: Vec::new(),
        });
    }
    posts
}

/// Download one page, retrying on network errors.
fn fetch_with_retries(client: &reqwest::blocking::Client, url: &str, max_retries: u32) -> Option<String> {
    for attempt in 1..=max_retries {
        println!("Scraping: {url}");
        let result = client.get(url).send().and_then(|r| r.error_for_status()).and_then(|r| r.text());
        match result {
            Ok(body) => return Some(body),
            Err(err) => {
                println!("Network error on {url} (attempt {attempt}/{max_retries}): {err}");
                if attempt < max_retries {
                    sleep(Duration::from_secs(3 * u64::from(attempt)));
                }
            }
        }
    }
    println!("FAILED page after retries: {url}");
    None
}

/// Scrape pages `1..=pages`, then save to `output_path` (backing up any
/// existing file first). `delay_secs` is the politeness pause between pages.
pub fn run(pages: u32, delay_secs: u64, output_path: &Path) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()?;

    let mut all_posts: Vec<RawPost> = Vec::new();
    let mut failed_pages: Vec<u32> = Vec::new();

    for page in 1..=pages {
        let url = if page == 1 { BASE_URL.to_string() } else { format!("{BASE_URL}page/{page}/") };
        match fetch_with_retries(&client, &url, 3) {
            Some(body) => {
                let found = parse_page(&body, &url);
                if found.is_empty() {
                    println!("WARNING: No replies found on this page: {url}");
                }
                all_posts.extend(found);
            }
            None => failed_pages.push(page),
        }
        sleep(Duration::from_secs(delay_secs));
    }

    println!(
        "Scrape summary: requested={pages}, failed_pages={}, extracted_posts={}",
        failed_pages.len(),
        all_posts.len()
    );
    if !failed_pages.is_empty() {
        println!("Failed page numbers: {failed_pages:?}");
    }
    save_posts(&all_posts, output_path)
}

/// Write posts as pretty JSON (4-space indent, like Python's `indent=4`),
/// copying any existing file to a timestamped backup first.
pub fn save_posts(posts: &[RawPost], output_path: &Path) -> Result<()> {
    if let Some(dir) = output_path.parent() {
        fs::create_dir_all(dir)?;
    }
    if output_path.exists() {
        let stem = output_path.file_stem().and_then(|s| s.to_str()).unwrap_or("data");
        let ext = output_path.extension().and_then(|s| s.to_str()).unwrap_or("json");
        let stamp = Local::now().format("%Y%m%d-%H%M%S");
        let backup = output_path.with_file_name(format!("{stem}.backup-{stamp}.{ext}"));
        fs::copy(output_path, &backup)?;
        println!("Backup created: {}", backup.display());
    }

    let mut buffer = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    serde::Serialize::serialize(posts, &mut serializer)?;
    fs::write(output_path, buffer)?;
    println!("Successfully saved {} posts to {}", posts.len(), output_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
    <html><body>
      <div class="bs-reply-list-item type-reply post-1496021 bbp-parent-topic-1367091">
        <a class="bbp-topic-permalink" href="/t">JP prep 2024/25</a>
        <span class="bs-timestamp">March 2, 2026 at 3:12 pm</span>
        <div class="bbp-reply-content">
          <p>Will be added when I diet.</p>
          <p>Second   paragraph</p>
          <div class="bbp-signature">-- my signature</div>
        </div>
      </div>
      <div class="bs-reply-list-item post-5">
        <div class="bbp-reply-content"><p>missing thread id</p></div>
      </div>
      <div class="bs-reply-list-item post-6 bbp-parent-topic-9">
        <span class="bs-timestamp">sometime last week</span>
        <div class="bbp-reply-content">bad date kept raw</div>
      </div>
    </body></html>"#;

    #[test]
    fn parses_ids_title_date_and_content() {
        let posts = parse_page(PAGE, "test");
        assert_eq!(posts.len(), 2); // the post without a thread id is skipped
        let p = &posts[0];
        assert_eq!(p.post_id, "1496021");
        assert_eq!(p.thread_id, "1367091");
        assert_eq!(p.thread_title.as_deref(), Some("JP prep 2024/25"));
        assert_eq!(p.timestamp.as_deref(), Some("2026-03-02T15:12:00"));
        assert_eq!(p.content.as_deref(), Some("Will be added when I diet.\nSecond   paragraph"));
    }

    #[test]
    fn signature_is_removed() {
        let posts = parse_page(PAGE, "test");
        assert!(!posts[0].content.as_deref().unwrap().contains("signature"));
    }

    #[test]
    fn unparseable_date_keeps_raw_string_with_null_timestamp() {
        let posts = parse_page(PAGE, "test");
        assert_eq!(posts[1].date.as_deref(), Some("sometime last week"));
        assert_eq!(posts[1].timestamp, None);
    }
}
