//! The searchable index: loading, saving and querying chunks.
//!
//! Python equivalent: `src/vector_store.py` (`TBJPVectorStore`, running in its
//! `backend="fallback"` mode). The Chroma / HuggingFace semantic backend is
//! deliberately not ported: the README already says the installation used
//! keyword search, and a Rust embedding stack would be a project of its own.
//! If you want semantic search later, the `fastembed` crate is the usual path.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use regex::Regex;

use crate::dates::{parse_forum_date, parse_iso};
use crate::models::{Chunk, Metadata};

/// How results are ordered (Python: the `sort_by` string).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Relevance,
    Newest,
    Oldest,
}

impl SortBy {
    /// Parse the strings the web form sends. Unknown text means relevance,
    /// matching the Python `else:` branch.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "newest" => SortBy::Newest,
            "oldest" => SortBy::Oldest,
            _ => SortBy::Relevance,
        }
    }
}

/// Lowercased `[a-z0-9]+` words of a string, as a set.
pub fn tokenize(value: &str) -> HashSet<String> {
    // `Regex::new` compiles the pattern; doing it once per call is fine here,
    // but see `Store::keyword_candidates` where it is hoisted out of a loop.
    word_regex()
        .find_iter(&value.to_lowercase())
        .map(|m| m.as_str().to_string())
        .collect()
}

fn word_regex() -> Regex {
    Regex::new(r"[a-z0-9]+").expect("static regex is valid")
}

/// Split a query into quoted phrases and bare words, ignoring the word `and`
/// (Python: `_parse_query_requirements`). Phrases are lowercased.
pub fn parse_query_requirements(query: &str) -> (Vec<String>, Vec<String>) {
    let quoted = Regex::new(r#""([^"]+)""#).expect("static regex is valid");
    let phrases: Vec<String> = quoted
        .captures_iter(query)
        .map(|c| c[1].trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();

    let remainder = quoted.replace_all(query, " ");
    let terms: Vec<String> = word_regex()
        .find_iter(&remainder.to_lowercase())
        .map(|m| m.as_str().to_string())
        .filter(|t| t != "and")
        .collect();
    (phrases, terms)
}

/// Does `text` contain every phrase and every whole word of the query?
pub fn matches_query_terms(text: &str, query: &str) -> bool {
    let (phrases, terms) = parse_query_requirements(query);
    if phrases.is_empty() && terms.is_empty() {
        return true;
    }
    let haystack = text.to_lowercase();
    if phrases.iter().any(|p| !haystack.contains(p.as_str())) {
        return false;
    }
    terms.iter().all(|term| {
        // `\b` = word boundary, so "mast" does not match "master".
        Regex::new(&format!(r"\b{}\b", regex::escape(term)))
            .map(|re| re.is_match(&haystack))
            .unwrap_or(false)
    })
}

/// A whole post rebuilt from its chunks, with ranking info.
struct PostGroup {
    chunk: Chunk,
    best_score: usize,
    first_rank: usize,
}

/// The in-memory index plus its file on disk.
pub struct Store {
    path: PathBuf,
    docs: Vec<Chunk>,
    /// Pre-computed word sets, one per chunk, so a search does not have to
    /// re-tokenise 15k posts every time. Always the same length as `docs`.
    tokens: Vec<HashSet<String>>,
}

impl Store {
    /// Open the index file (an empty store if it does not exist yet).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut docs = Vec::new();
        if path.exists() {
            let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
            for (n, line) in BufReader::new(file).lines().enumerate() {
                let line = line?;
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                // `?` returns early with the error if parsing fails. The
                // `with_context` text tells you which line was bad.
                let chunk: Chunk = serde_json::from_str(line)
                    .with_context(|| format!("{}: line {} is not valid index JSON", path.display(), n + 1))?;
                docs.push(chunk);
            }
        }
        let tokens = docs.iter().map(|d| tokenize(&d.page_content)).collect();
        Ok(Self { path, docs, tokens })
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Add chunks, replacing any existing chunks of the same posts, then save.
    ///
    /// Behaviour change from Python: running `ingest.py` twice appended every
    /// chunk twice. Replacing by `post_id` makes ingesting repeatable, and lets
    /// `seed` add sample posts without wiping a real index.
    pub fn upsert(&mut self, chunks: Vec<Chunk>) -> Result<()> {
        let incoming: HashSet<&str> = chunks
            .iter()
            .filter_map(|c| c.metadata.post_id.as_deref())
            .collect();
        self.docs
            .retain(|d| d.metadata.post_id.as_deref().is_none_or(|id| !incoming.contains(id)));
        self.docs.extend(chunks);
        self.tokens = self.docs.iter().map(|d| tokenize(&d.page_content)).collect();
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = BufWriter::new(File::create(&self.path)?);
        for doc in &self.docs {
            serde_json::to_writer(&mut out, doc)?;
            out.write_all(b"\n")?;
        }
        out.flush()?;
        Ok(())
    }

    /// Chunks sharing at least one word with the query, best match first.
    /// Score = number of shared words (Python: `_KeywordRetriever.invoke`).
    /// Returns at most `top_k` hits; `top_k == 0` means "no limit".
    fn keyword_candidates(&self, query: &str, top_k: usize) -> Vec<&Chunk> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, &Chunk)> = self
            .docs
            .iter()
            .zip(&self.tokens)
            .filter_map(|(doc, words)| {
                let score = query_terms.intersection(words).count();
                (score > 0).then_some((score, doc))
            })
            .collect();
        // Stable sort, so equal scores stay in index order (like Python).
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let limit = if top_k == 0 { scored.len() } else { top_k };
        scored.into_iter().take(limit).map(|(_, doc)| doc).collect()
    }

    /// Search used by the UI and CLI (Python: `hybrid_search`).
    ///
    /// 1. Over-fetch keyword candidates (5x `k`, or everything if `k == 0`).
    /// 2. Re-assemble whole posts from their chunks.
    /// 3. If `exact_match`, keep only posts containing every phrase/word.
    /// 4. Sort and cut to `k` (`k == 0` = all).
    pub fn hybrid_search(&self, query: &str, k: usize, exact_match: bool, sort_by: SortBy) -> Vec<Chunk> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let candidate_k = if k == 0 { 0 } else { k * 5 };
        let candidates = self.keyword_candidates(query, candidate_k);
        if candidates.is_empty() {
            return Vec::new();
        }

        // In Python the "score" of a keyword hit was just its rank (0, 1, 2…),
        // so ordering by score and by rank are the same thing here.
        let mut groups = rehydrate_posts(&candidates);

        if exact_match {
            groups.retain(|g| {
                let title = g.chunk.metadata.thread_title.as_deref().unwrap_or("");
                matches_query_terms(&format!("{} {}", title, g.chunk.page_content), query)
            });
        }

        match sort_by {
            SortBy::Newest => groups.sort_by(|a, b| chunk_datetime(&b.chunk).cmp(&chunk_datetime(&a.chunk))),
            SortBy::Oldest => groups.sort_by(|a, b| chunk_datetime(&a.chunk).cmp(&chunk_datetime(&b.chunk))),
            SortBy::Relevance => groups.sort_by_key(|g| (g.best_score, g.first_rank)),
        }

        let limit = if k == 0 { groups.len() } else { k };
        groups.into_iter().take(limit).map(|g| g.chunk).collect()
    }
}

/// Group chunk hits by `post_id` and glue each post's distinct chunks back
/// together (Python: `_rehydrate_posts`). Order of first appearance is kept.
fn rehydrate_posts(candidates: &[&Chunk]) -> Vec<PostGroup> {
    let mut groups: Vec<PostGroup> = Vec::new();
    let mut index_by_post: HashMap<String, usize> = HashMap::new();
    let mut seen_chunks: Vec<HashSet<String>> = Vec::new();
    let mut missing_counter = 0usize;

    for (rank, doc) in candidates.iter().enumerate() {
        let post_id = match doc.metadata.post_id.as_deref().map(str::trim) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => {
                missing_counter += 1;
                format!("_missing_post_{}", missing_counter - 1)
            }
        };

        let slot = *index_by_post.entry(post_id).or_insert_with(|| {
            groups.push(PostGroup {
                chunk: Chunk { page_content: String::new(), metadata: doc.metadata.clone() },
                best_score: rank,
                first_rank: rank,
            });
            seen_chunks.push(HashSet::new());
            groups.len() - 1
        });

        let text = doc.page_content.trim();
        if !text.is_empty() && seen_chunks[slot].insert(text.to_string()) {
            let content = &mut groups[slot].chunk.page_content;
            if !content.is_empty() {
                content.push_str("\n\n");
            }
            content.push_str(text);
        }
    }
    groups
}

/// When was this post written? Falls back to the oldest possible date so
/// undated posts sort last in "newest" and first in "oldest".
fn chunk_datetime(chunk: &Chunk) -> NaiveDateTime {
    let Metadata { timestamp, date, .. } = &chunk.metadata;
    timestamp
        .as_deref()
        .and_then(parse_iso)
        .or_else(|| date.as_deref().and_then(parse_forum_date))
        .unwrap_or(NaiveDateTime::MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(post_id: &str, title: &str, text: &str, ts: &str) -> Chunk {
        Chunk {
            page_content: text.to_string(),
            metadata: Metadata {
                post_id: Some(post_id.into()),
                thread_title: Some(title.into()),
                timestamp: Some(ts.into()),
                ..Default::default()
            },
        }
    }

    fn store(docs: Vec<Chunk>) -> Store {
        let tokens = docs.iter().map(|d| tokenize(&d.page_content)).collect();
        Store { path: PathBuf::from("unused"), docs, tokens }
    }

    fn sample() -> Store {
        store(vec![
            chunk("1", "Hamstrings", "Stiff leg deadlift for hamstrings", "2026-01-01T10:00:00"),
            chunk("2", "Chest", "Incline press for upper chest", "2026-03-01T10:00:00"),
            chunk("3", "Legs", "Leg curl and stiff leg work", "2026-02-01T10:00:00"),
        ])
    }

    #[test]
    fn tokenize_lowercases_and_splits() {
        let t = tokenize("Hack-Squat, 3x10!");
        assert!(t.contains("hack") && t.contains("squat") && t.contains("3x10"));
    }

    #[test]
    fn query_requirements_split_phrases_and_terms() {
        let (phrases, terms) = parse_query_requirements(r#""Stiff Leg" and rdl"#);
        assert_eq!(phrases, vec!["stiff leg"]);
        assert_eq!(terms, vec!["rdl"]);
    }

    #[test]
    fn exact_match_uses_whole_words() {
        assert!(matches_query_terms("the test and mast cycle", "test mast"));
        assert!(!matches_query_terms("the master plan", "mast"));
        assert!(matches_query_terms("a stiff leg day", r#""stiff leg""#));
        assert!(!matches_query_terms("stiff day leg", r#""stiff leg""#));
    }

    #[test]
    fn ranks_by_shared_words() {
        let results = sample().hybrid_search("stiff leg deadlift", 10, false, SortBy::Relevance);
        assert_eq!(results[0].metadata.post_id.as_deref(), Some("1"));
        assert_eq!(results.len(), 2); // the chest post shares no word
    }

    #[test]
    fn sorts_by_date() {
        let s = sample();
        let newest = s.hybrid_search("leg", 10, false, SortBy::Newest);
        assert_eq!(newest[0].metadata.post_id.as_deref(), Some("3"));
        let oldest = s.hybrid_search("leg", 10, false, SortBy::Oldest);
        assert_eq!(oldest[0].metadata.post_id.as_deref(), Some("1"));
    }

    #[test]
    fn chunks_of_one_post_are_rejoined_once() {
        let s = store(vec![
            chunk("9", "T", "alpha beta part one", "2026-01-01T00:00:00"),
            chunk("9", "T", "alpha part two", "2026-01-01T00:00:00"),
            chunk("9", "T", "alpha part two", "2026-01-01T00:00:00"), // duplicate
        ]);
        let results = s.hybrid_search("alpha", 10, false, SortBy::Relevance);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page_content, "alpha beta part one\n\nalpha part two");
    }

    #[test]
    fn empty_query_returns_nothing() {
        assert!(sample().hybrid_search("   ", 10, false, SortBy::Relevance).is_empty());
    }

    #[test]
    fn upsert_replaces_by_post_id_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("tbjp-store-test-{}", std::process::id()));
        let path = dir.join("docs.jsonl");
        let mut s = Store::open(&path).unwrap();
        s.upsert(vec![chunk("1", "A", "first version", "2026-01-01T00:00:00")]).unwrap();
        s.upsert(vec![chunk("1", "A", "second version", "2026-01-01T00:00:00")]).unwrap();
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened.docs[0].page_content, "second version");
        fs::remove_dir_all(dir).ok();
    }
}
