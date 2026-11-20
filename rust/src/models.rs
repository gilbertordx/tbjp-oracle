//! Data types shared by every other module.
//!
//! Python equivalents: `src/schema.py` (pydantic `ForumPost`) and the
//! LangChain `Document` class that the Python code passed around.
//!
//! In Python these were a pydantic model and a dict-of-metadata. In Rust we
//! describe each shape once as a `struct`; the `serde` derive macros then
//! generate all JSON reading and writing for us.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// One record exactly as it appears in `data/raw_forum_data.json`
/// (the scraper's output). Everything that can be missing or `null` in the
/// JSON is an `Option`, which is Rust's way of saying "this might not exist".
/// The compiler then forces every caller to handle the missing case.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawPost {
    pub post_id: String,
    pub thread_id: String,
    pub thread_title: Option<String>,
    pub author: Option<String>,
    /// ISO timestamp, e.g. `2026-03-02T15:12:00`.
    pub timestamp: Option<String>,
    /// Human date scraped from the page, e.g. `March 2, 2026 at 3:12 pm`.
    pub date: Option<String>,
    pub content: Option<String>,
    pub parent_post_id: Option<String>,
    /// `#[serde(default)]` = "if the key is absent, use an empty Vec".
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A validated post (Python: pydantic `ForumPost`). Unlike `RawPost`, the
/// timestamp is guaranteed to exist and to be a real date.
#[derive(Debug, Clone)]
pub struct ForumPost {
    pub post_id: String,
    pub thread_id: String,
    pub author: String,
    pub timestamp: NaiveDateTime,
    pub content: String,
    pub parent_post_id: Option<String>,
    pub tags: Vec<String>,
}

/// Source information attached to every chunk (Python: `Document.metadata`,
/// which was an untyped dict). Here each key is a named, typed field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// ISO timestamp of the post.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Human-readable date from the forum page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_title: Option<String>,
}

/// A piece of text plus where it came from (Python: LangChain `Document`).
/// This is also the line format of the on-disk index, `docs.jsonl`:
/// `{"page_content": "...", "metadata": {...}}` -- identical to what the
/// Python fallback store wrote, so old index files still load.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Chunk {
    pub page_content: String,
    #[serde(default)]
    pub metadata: Metadata,
}
