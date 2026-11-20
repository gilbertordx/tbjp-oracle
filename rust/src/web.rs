//! The browser UI. Python equivalent: `app.py` (Streamlit).
//!
//! Streamlit re-runs the whole script on every click and keeps state in
//! `st.session_state`. A normal web server has no such magic, so the state
//! (query, filters, page) lives in the URL's query string instead. That also
//! means every search result page is bookmarkable and the Back button works.
//!
//! HTML is built with plain `format!` and a small `escape` function, which
//! keeps the dependency list short. Everything user-supplied goes through
//! `escape` first (otherwise a query like `<script>` would run in the page).

use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use regex::{Regex, RegexBuilder};
use serde::Deserialize;

use crate::llm::ArchiveLlm;
use crate::models::Chunk;
use crate::store::{SortBy, Store, parse_query_requirements};

/// Shared, read-only state handed to every request handler.
/// `Arc` = "atomically reference counted": many handlers share one `Store`.
pub struct AppState {
    pub store: Store,
    pub llm: ArchiveLlm,
}

pub async fn serve(state: AppState, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(search_page))
        .route("/rag", get(rag_page))
        .with_state(Arc::new(state));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("ARCHIVE running at http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Replace the five HTML-special characters (Python: `html.escape`).
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Wrap query matches in `<mark>` (Python: `highlight_text`). The text is
/// escaped first, then matched, then newlines become `<br>`.
pub fn highlight_text(text: &str, query: &str) -> String {
    let escaped = escape(text);
    if query.trim().is_empty() {
        return escaped.replace('\n', "<br>");
    }

    // Unlike the store, the UI keeps the phrase's original case; matching is
    // case-insensitive anyway.
    let (phrases, terms) = {
        let (_, terms) = parse_query_requirements(query);
        let quoted = Regex::new(r#""([^"]+)""#).expect("static regex");
        let phrases: Vec<String> = quoted
            .captures_iter(query)
            .map(|c| c[1].trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        (phrases, terms)
    };

    let mut parts: Vec<String> = phrases.iter().map(|p| regex::escape(p)).collect();
    parts.extend(terms.iter().map(|t| format!(r"\b{}\b", regex::escape(t))));
    if parts.is_empty() {
        return escaped.replace('\n', "<br>");
    }
    // Longest first so a phrase wins over the single words inside it.
    parts.sort_by_key(|p| std::cmp::Reverse(p.len()));

    let combined = RegexBuilder::new(&format!("({})", parts.join("|")))
        .case_insensitive(true)
        .build()
        .expect("escaped parts always form a valid regex");
    combined.replace_all(&escaped, "<mark>${1}</mark>").replace('\n', "<br>")
}

const STYLE: &str = r#"
:root{--bg:#f6f7f9;--card:#fff;--ink:#1b1f24;--muted:#6a7380;--line:#e3e6eb;--accent:#d9480f;--accent-ink:#fff;--mark:#ffe08a;--note:#e8f0ff;--warn:#fff1de}
@media (prefers-color-scheme:dark){:root{--bg:#0f1216;--card:#171b21;--ink:#e8eaed;--muted:#8f98a5;--line:#262c34;--accent:#ff7a3d;--accent-ink:#1a1006;--mark:#6b5300;--note:#16233a;--warn:#33240f}}
*{box-sizing:border-box}
body{font-family:system-ui,-apple-system,"Segoe UI",sans-serif;background:var(--bg);color:var(--ink);max-width:900px;margin:0 auto;padding:0 1rem 4rem;line-height:1.55}
header{display:flex;align-items:baseline;gap:1rem;flex-wrap:wrap;padding:1.6rem 0 1rem}
header h1{margin:0;font-size:1.7rem;letter-spacing:-.02em}header h1 span{color:var(--accent)}
header p{margin:0;color:var(--muted);flex:1;min-width:14rem}
header nav a,header nav strong{padding:.3rem .7rem;border-radius:999px;font-size:.9rem;text-decoration:none}
header nav a{color:var(--muted);border:1px solid var(--line)}header nav a:hover{color:var(--ink)}
header nav strong{background:var(--accent);color:var(--accent-ink)}
form.row{display:flex;gap:.6rem;flex-wrap:wrap;align-items:flex-end;background:var(--card);border:1px solid var(--line);border-radius:12px;padding:.9rem;margin:1rem 0}
label{display:flex;flex-direction:column;font-size:.72rem;text-transform:uppercase;letter-spacing:.04em;color:var(--muted);gap:.25rem}
select,input[type=text]{font:inherit;color:var(--ink);background:var(--bg);border:1px solid var(--line);border-radius:8px;padding:.5rem .6rem}
input[type=text]{flex:1;min-width:16rem;font-size:1rem}
select:focus,input:focus{outline:2px solid var(--accent);outline-offset:1px}
button{font:inherit;font-weight:600;background:var(--accent);color:var(--accent-ink);border:0;border-radius:8px;padding:.55rem 1.2rem;cursor:pointer}
button:hover{filter:brightness(1.08)}
details{background:var(--card);border:1px solid var(--line);border-radius:12px;margin:.6rem 0;padding:.7rem 1rem}
details[open]{border-color:var(--accent)}
summary{cursor:pointer;font-weight:600;list-style:none}summary::-webkit-details-marker{display:none}
summary::before{content:"\25B8";color:var(--accent);margin-right:.5rem}details[open] summary::before{content:"\25BE"}
details hr{border:0;border-top:1px solid var(--line);margin:.7rem 0}
details div{white-space:normal;overflow-wrap:anywhere}
mark{background:var(--mark);color:inherit;border-radius:3px;padding:0 .1em}
nav.pager{display:flex;gap:1rem;align-items:center;justify-content:space-between;margin:1rem 0;color:var(--muted);font-size:.9rem}
nav.pager a{color:var(--accent);text-decoration:none;font-weight:600}
.note,.warn{padding:.6rem .9rem;border-radius:10px;font-size:.9rem;margin:.6rem 0}
.note{background:var(--note)}.warn{background:var(--warn)}
code{background:var(--line);padding:.1em .35em;border-radius:4px}
a{color:var(--accent)}h3{margin-top:1.6rem}
"#;

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><style>{STYLE}</style></head><body>{body}</body></html>",
        escape(title)
    ))
}

fn nav(active: &str) -> String {
    let link = |href: &str, label: &str| {
        if label == active { format!("<strong>{label}</strong>") } else { format!("<a href=\"{href}\">{label}</a>") }
    };
    format!(
        "<header><h1>arch<span>ive</span></h1><p>search jordan peters\u{2019} forum archive</p><nav>{} {}</nav></header>",
        link("/", "search"),
        link("/rag", "rag mode *experimental")
    )
}

#[derive(Debug, Deserialize, Default)]
pub struct SearchParams {
    q: Option<String>,
    scope: Option<String>,
    sort: Option<String>,
    size: Option<usize>,
    exact: Option<String>,
    page: Option<usize>,
}

/// "all matches" -> 0 (no limit), "top 100" -> 100 (Python: `scope_to_k`).
fn scope_to_k(scope: &str) -> usize {
    scope.strip_prefix("top ").and_then(|n| n.parse().ok()).unwrap_or(0)
}

fn select(name: &str, label: &str, options: &[&str], selected: &str) -> String {
    let opts: String = options
        .iter()
        .map(|o| format!("<option{}>{}</option>", if *o == selected { " selected" } else { "" }, escape(o)))
        .collect();
    format!("<label>{label}<select name=\"{name}\">{opts}</select></label>")
}

fn render_document(doc: &Chunk, query: &str) -> String {
    let m = &doc.metadata;
    let id = m.post_id.as_deref().unwrap_or("unknown id");
    let date = m.date.as_deref().unwrap_or("unknown date");
    let title = m.thread_title.as_deref().unwrap_or("unknown thread");
    format!(
        "<details><summary>{} | {} (id: {})</summary><p><strong>thread:</strong> {}</p><hr><div>{}</div></details>",
        escape(date),
        escape(title),
        escape(id),
        highlight_text(title, query).replace("<br>", ""),
        highlight_text(&doc.page_content, query),
    )
}

async fn search_page(State(state): State<Arc<AppState>>, Query(p): Query<SearchParams>) -> Html<String> {
    let query = p.q.clone().unwrap_or_default();
    let scope = p.scope.clone().unwrap_or_else(|| "all matches".into());
    let sort = p.sort.clone().unwrap_or_else(|| "relevance".into());
    let size = p.size.filter(|s| *s > 0).unwrap_or(20);
    let exact = p.exact.as_deref() == Some("on");
    let page_index = p.page.unwrap_or(0);

    let mut body = nav("search");
    if state.store.is_empty() {
        body.push_str("<p class=\"warn\">the index is empty. run <code>tbjp-oracle ingest</code> (or <code>seed</code>) first.</p>");
    }
    body.push_str("<p class=\"note\">keyword search is active. enable exact match to require every word or quoted phrase.</p>");

    body.push_str(&format!(
        "<form class=\"row\" method=\"get\" action=\"/\">{}{}{}{}\
         <input type=\"text\" name=\"q\" value=\"{}\" placeholder=\"use quotes for exact phrase: &quot;trap bar&quot; rdl\">\
         <button type=\"submit\">search</button></form>",
        select("scope", "result scope", &["all matches", "top 50", "top 100", "top 200", "top 500"], &scope),
        select("sort", "sort", &["relevance", "newest", "oldest"], &sort),
        select("size", "page size", &["10", "20", "50", "100"], &size.to_string()),
        select("exact", "exact match", &["off", "on"], if exact { "on" } else { "off" }),
        escape(&query),
    ));

    if query.trim().is_empty() {
        return page("archive", &body);
    }

    let results = state
        .store
        .hybrid_search(&query, scope_to_k(&scope), exact, SortBy::from_str_lossy(&sort));
    let total = results.len();
    if total == 0 {
        body.push_str("<p class=\"warn\">no matching entries found for this query.</p>");
        return page("archive", &body);
    }

    let total_pages = total.div_ceil(size).max(1);
    let page_index = page_index.min(total_pages - 1);
    let start = page_index * size;
    let end = (start + size).min(total);

    // Keep every filter in the prev/next links so paging does not reset them.
    let link = |target: usize, label: &str| {
        format!(
            "<a href=\"/?q={}&scope={}&sort={}&size={}&exact={}&page={target}\">{label}</a>",
            url_encode(&query),
            url_encode(&scope),
            url_encode(&sort),
            size,
            if exact { "on" } else { "off" },
        )
    };
    body.push_str(&format!(
        "<nav class=\"pager\">{}<span><strong>showing {}-{} of {}</strong> (page {}/{})</span>{}</nav>",
        if page_index > 0 { link(page_index - 1, "previous") } else { String::new() },
        start + 1,
        end,
        total,
        page_index + 1,
        total_pages,
        if page_index + 1 < total_pages { link(page_index + 1, "next") } else { String::new() },
    ));
    for doc in &results[start..end] {
        body.push_str(&render_document(doc, &query));
    }
    page("archive", &body)
}

#[derive(Debug, Deserialize, Default)]
pub struct RagParams {
    q: Option<String>,
}

async fn rag_page(State(state): State<Arc<AppState>>, Query(p): Query<RagParams>) -> Html<String> {
    let question = p.q.unwrap_or_default();
    let mut body = nav("rag mode *experimental");
    body.push_str("<p class=\"warn\">experimental feature: ai answers may be incomplete or inaccurate. verify claims against the original forum posts.</p>");

    let available = state.llm.available().await;
    if !available {
        body.push_str(&format!(
            "<p class=\"note\">install the free local model first: <code>ollama pull {}</code></p>",
            escape(&state.llm.model)
        ));
    }
    body.push_str(&format!(
        "<form class=\"row\" method=\"get\" action=\"/rag\"><input type=\"text\" name=\"q\" value=\"{}\" \
         placeholder=\"ask about training, nutrition, or programming\"><button type=\"submit\">ask</button></form>",
        escape(&question)
    ));

    if !question.trim().is_empty() {
        let documents = state.store.hybrid_search(&question, 6, false, SortBy::Relevance);
        let answer = if documents.is_empty() {
            "I couldn't find enough relevant replies in the archive to answer that.".to_string()
        } else if !available {
            format!("ollama is not ready. install the local model with: ollama pull {}", state.llm.model)
        } else {
            match state.llm.answer(&question, &documents).await {
                Ok(text) => text,
                Err(err) => format!("rag request failed: {err}"),
            }
        };
        body.push_str(&format!("<h3>answer</h3><div>{}</div>", escape(&answer).replace('\n', "<br>")));
    }
    page("archive - rag", &body)
}

/// Percent-encode everything except unreserved characters, for use in URLs.
fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_neutralises_html() {
        assert_eq!(escape("<b>\"hi\" & 'bye'</b>"), "&lt;b&gt;&quot;hi&quot; &amp; &#x27;bye&#x27;&lt;/b&gt;");
    }

    #[test]
    fn highlights_words_and_phrases_case_insensitively() {
        let out = highlight_text("Stiff Leg deadlifts and RDL work", r#""stiff leg" rdl"#);
        assert_eq!(out, "<mark>Stiff Leg</mark> deadlifts and <mark>RDL</mark> work");
    }

    #[test]
    fn highlight_does_not_match_inside_words() {
        assert_eq!(highlight_text("the master", "mast"), "the master");
    }

    #[test]
    fn highlight_escapes_the_page_text() {
        assert_eq!(highlight_text("<script>", "script"), "&lt;<mark>script</mark>&gt;");
    }

    #[test]
    fn scope_strings_map_to_limits() {
        assert_eq!(scope_to_k("all matches"), 0);
        assert_eq!(scope_to_k("top 100"), 100);
    }

    #[test]
    fn url_encoding_keeps_filters_intact() {
        assert_eq!(url_encode("\"trap bar\" rdl"), "%22trap%20bar%22%20rdl");
    }
}
