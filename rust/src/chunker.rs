//! Splits long posts into overlapping chunks.
//!
//! Python equivalent: `src/pipeline.py`, which used LangChain's
//! `RecursiveCharacterTextSplitter(chunk_size=2000, chunk_overlap=200,
//! separators=["\n\n", "\n", ".", " "])`.
//!
//! There is no LangChain in Rust, so this is a faithful port of that one
//! algorithm (~60 lines) instead of a 50-package dependency. The algorithm:
//!
//! 1. Split the text on the first separator that occurs in it, keeping the
//!    separator attached to the *start* of the following piece.
//! 2. Pieces shorter than the chunk size are merged back together (with
//!    overlap) into chunks.
//! 3. Pieces that are still too long are split again with the *next*
//!    separator (that is the "recursive" part).
//!
//! Lengths are counted in characters, not bytes, because Python's `len()`
//! counts characters and the forum text contains curly quotes and dashes
//! that take several bytes each in UTF-8.

use crate::models::{Chunk, ForumPost, Metadata};

pub const CHUNK_SIZE: usize = 2000;
pub const CHUNK_OVERLAP: usize = 200;
pub const SEPARATORS: [&str; 4] = ["\n\n", "\n", ".", " "];

/// Number of characters (Python `len(str)`), not bytes (Rust `str::len`).
fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Split `text` on `separator`, but keep the separator, glued to the start
/// of the piece that follows it. Empty pieces are dropped.
///
/// `&str` is a *borrowed* view of text; returning `Vec<String>` gives the
/// caller brand-new owned strings so nothing here has to outlive `text`.
fn split_keep_separator(text: &str, separator: &str) -> Vec<String> {
    let mut pieces = text.split(separator);
    let mut out: Vec<String> = Vec::new();
    if let Some(first) = pieces.next() {
        out.push(first.to_string());
    }
    for piece in pieces {
        out.push(format!("{separator}{piece}"));
    }
    out.retain(|s| !s.is_empty());
    out
}

/// Join small pieces into chunks of at most `CHUNK_SIZE` characters, carrying
/// up to `CHUNK_OVERLAP` characters from the end of one chunk into the next.
fn merge_splits(splits: &[String], separator: &str) -> Vec<String> {
    let separator_len = char_len(separator);
    let mut docs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut total = 0usize;

    for piece in splits {
        let piece_len = char_len(piece);
        let sep_cost = |n: usize| if n > 0 { separator_len } else { 0 };

        if total + piece_len + sep_cost(current.len()) > CHUNK_SIZE {
            if !current.is_empty() {
                let doc = current.join(separator).trim().to_string();
                if !doc.is_empty() {
                    docs.push(doc);
                }
                // Drop pieces from the front until we are under the overlap
                // budget and the new piece fits.
                while total > CHUNK_OVERLAP
                    || (total + piece_len + sep_cost(current.len()) > CHUNK_SIZE && total > 0)
                {
                    let removed = char_len(current[0]) + if current.len() > 1 { separator_len } else { 0 };
                    total -= removed;
                    current.remove(0);
                }
            }
        }
        current.push(piece);
        total += piece_len + if current.len() > 1 { separator_len } else { 0 };
    }

    let doc = current.join(separator).trim().to_string();
    if !doc.is_empty() {
        docs.push(doc);
    }
    docs
}

/// The recursive part: try the first separator that is present in `text`.
fn split_text_with(text: &str, separators: &[&str]) -> Vec<String> {
    // Pick the first separator found in the text. If none is found, fall back
    // to the last one with nothing left to recurse into.
    let mut separator = separators[separators.len() - 1];
    let mut remaining: &[&str] = &[];
    for (i, s) in separators.iter().enumerate() {
        if text.contains(s) {
            separator = s;
            remaining = &separators[i + 1..];
            break;
        }
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut good: Vec<String> = Vec::new(); // pieces small enough to merge

    for piece in split_keep_separator(text, separator) {
        if char_len(&piece) < CHUNK_SIZE {
            good.push(piece);
            continue;
        }
        if !good.is_empty() {
            // The separator is already stuck to the pieces, so merge with "".
            chunks.extend(merge_splits(&good, ""));
            good.clear();
        }
        if remaining.is_empty() {
            chunks.push(piece);
        } else {
            chunks.extend(split_text_with(&piece, remaining));
        }
    }
    if !good.is_empty() {
        chunks.extend(merge_splits(&good, ""));
    }
    chunks
}

/// Split one text into chunks using the project's settings.
pub fn split_text(text: &str) -> Vec<String> {
    split_text_with(text, &SEPARATORS)
}

/// Turn a validated post into searchable chunks (Python: `process_forum_post`).
/// Blank posts produce no chunks. Every chunk carries the post's metadata.
pub fn process_forum_post(post: &ForumPost) -> Vec<Chunk> {
    if post.content.trim().is_empty() {
        return Vec::new();
    }
    split_text(&post.content)
        .into_iter()
        .map(|page_content| Chunk {
            page_content,
            metadata: Metadata {
                post_id: Some(post.post_id.clone()),
                thread_id: Some(post.thread_id.clone()),
                author: Some(post.author.clone()),
                timestamp: Some(post.timestamp.format("%Y-%m-%dT%H:%M:%S").to_string()),
                date: None,
                thread_title: None,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_a_single_chunk() {
        assert_eq!(split_text("hello world"), vec!["hello world"]);
    }

    #[test]
    fn blank_text_gives_no_chunks() {
        assert!(split_text("   ").is_empty());
    }

    #[test]
    fn long_text_is_split_and_every_chunk_fits() {
        let sentence = "Stiff leg deadlifts build the hamstrings. ";
        let text = sentence.repeat(200); // ~8400 chars
        let chunks = split_text(&text);
        assert!(chunks.len() > 3);
        assert!(chunks.iter().all(|c| char_len(c) <= CHUNK_SIZE));
    }

    #[test]
    fn counts_characters_not_bytes() {
        // 1500 curly quotes = 4500 bytes but only 1500 characters: one chunk.
        let text = "\u{2019}".repeat(1500);
        assert_eq!(split_text(&text).len(), 1);
    }
}
