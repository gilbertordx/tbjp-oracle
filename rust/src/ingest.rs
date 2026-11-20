//! Turns the scraped JSON into index chunks, and builds the sample data.
//!
//! Python equivalents: `ingest.py` and `mock_data_gen.py`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Duration, Local, NaiveDateTime};

use crate::chunker::process_forum_post;
use crate::dates::{parse_forum_date, parse_iso};
use crate::models::{Chunk, ForumPost, RawPost};

/// Best-effort timestamp: the ISO field first, then the human date.
pub fn parse_timestamp(item: &RawPost) -> Option<NaiveDateTime> {
    item.timestamp
        .as_deref()
        .and_then(parse_iso)
        .or_else(|| item.date.as_deref().and_then(parse_forum_date))
}

/// Chunk every post that has a usable date. Returns the chunks and how many
/// posts were skipped (we never invent a date; see AGENTS.md rule 1).
pub fn build_chunks(raw: &[RawPost]) -> (Vec<Chunk>, usize) {
    let mut chunks = Vec::new();
    let mut skipped = 0usize;

    for item in raw {
        let Some(timestamp) = parse_timestamp(item) else {
            skipped += 1;
            println!(
                "Skipping post due to invalid/missing timestamp and date: post_id={}, date={}",
                item.post_id,
                item.date.as_deref().unwrap_or("")
            );
            continue;
        };

        let author = item.author.clone().unwrap_or_else(|| "Jordan Peters".to_string());
        let post = ForumPost {
            post_id: item.post_id.clone(),
            thread_id: item.thread_id.clone(),
            author: author.clone(),
            timestamp,
            content: item.content.clone().unwrap_or_default(),
            parent_post_id: item.parent_post_id.clone(),
            tags: item.tags.clone(),
        };

        for mut chunk in process_forum_post(&post) {
            chunk.metadata.date = Some(item.date.clone().unwrap_or_else(|| "Unknown Date".to_string()));
            chunk.metadata.thread_title =
                Some(item.thread_title.clone().unwrap_or_else(|| "Unknown Thread".to_string()));
            chunks.push(chunk);
        }
    }
    (chunks, skipped)
}

/// Read `raw_forum_data.json`.
pub fn load_raw_posts(path: &Path) -> Result<Vec<RawPost>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Three hand-written sample posts so you can try the app without scraping.
pub fn mock_posts() -> Vec<ForumPost> {
    let now = Local::now().naive_local();
    vec![
        ForumPost {
            post_id: "p001".into(),
            thread_id: "t_deadlift_swaps".into(),
            author: "coach_j".into(),
            timestamp: now - Duration::days(2),
            content: "If conventional deadlifts are crushing your recovery, stop forcing them. \
                      Use RDLs as your hinge pattern, and add a 45-degree back extension for \
                      high-rep posterior chain volume. If you still want a heavy pull, run a \
                      Smith machine block pull from just below the knee."
                .into(),
            parent_post_id: None,
            tags: vec!["deadlift".into(), "posterior-chain".into(), "exercise-selection".into()],
        },
        ForumPost {
            post_id: "p002".into(),
            thread_id: "t_deadlift_swaps".into(),
            author: "coach_j".into(),
            timestamp: now - Duration::days(1) - Duration::hours(10),
            content: "For hypertrophy clients, I rarely need conventional deadlifts. \
                      A leg curl + RDL + back extension stack usually beats it for stimulus \
                      with less axial fatigue."
                .into(),
            parent_post_id: Some("p001".into()),
            tags: vec!["hypertrophy".into(), "fatigue-management".into()],
        },
        ForumPost {
            post_id: "p003".into(),
            thread_id: "t_squat".into(),
            author: "user_anon_22".into(),
            timestamp: now - Duration::days(1),
            content: "My knees cave on hack squat when load gets heavy. \
                      Any cues for bracing and foot pressure?"
                .into(),
            parent_post_id: None,
            tags: vec!["hack-squat".into(), "technique".into()],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(id: &str, ts: Option<&str>, date: Option<&str>, content: &str) -> RawPost {
        RawPost {
            post_id: id.into(),
            thread_id: "t1".into(),
            timestamp: ts.map(String::from),
            date: date.map(String::from),
            content: Some(content.into()),
            ..Default::default()
        }
    }

    #[test]
    fn falls_back_to_the_human_date() {
        let item = raw("1", None, Some("March 2, 2026 at 3:12 pm"), "x");
        assert_eq!(parse_timestamp(&item).unwrap().to_string(), "2026-03-02 15:12:00");
    }

    #[test]
    fn skips_posts_without_any_date() {
        let (chunks, skipped) = build_chunks(&[raw("1", None, None, "hello"), raw("2", Some("2026-01-01T00:00:00"), None, "hi")]);
        assert_eq!(skipped, 1);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].metadata.post_id.as_deref(), Some("2"));
    }

    #[test]
    fn fills_defaults_for_missing_title_and_date() {
        let (chunks, _) = build_chunks(&[raw("1", Some("2026-01-01T00:00:00"), None, "hello")]);
        assert_eq!(chunks[0].metadata.thread_title.as_deref(), Some("Unknown Thread"));
        assert_eq!(chunks[0].metadata.date.as_deref(), Some("Unknown Date"));
        assert_eq!(chunks[0].metadata.author.as_deref(), Some("Jordan Peters"));
    }

    #[test]
    fn blank_posts_make_no_chunks() {
        let (chunks, skipped) = build_chunks(&[raw("1", Some("2026-01-01T00:00:00"), None, "   ")]);
        assert!(chunks.is_empty());
        assert_eq!(skipped, 0);
    }
}
