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
*{box-sizing:border-box;border-radius:0!important}
html[data-theme=dune]{--bg:#ecdcb9;--surface:#f4e9cf;--ink:#2a1b10;--muted:#7a6347;--line:#c9b48a;--strong:#2a1b10;--accent:#b8470f;--accent-ink:#f8ecd2;--link:#1d4f7a;--mark:#e8a33d;--mark-ink:#2a1b10}
@media (prefers-color-scheme:dark){html[data-theme=dune]{--bg:#15100b;--surface:#1e1610;--ink:#ead9b8;--muted:#9a8460;--line:#4a3826;--strong:#ead9b8;--accent:#f08a2b;--accent-ink:#15100b;--link:#6ab0e0;--mark:#b4651c;--mark-ink:#fff4de}}
html[data-theme=kali]{--bg:#0a0c10;--surface:#0a0c10;--ink:#c9d1d9;--muted:#6e7681;--line:#21262d;--strong:#c9d1d9;--accent:#3b8eea;--accent-ink:#0a0c10;--link:#3b8eea;--mark:#e3b341;--mark-ink:#0a0c10}
body{margin:0;background:var(--bg);color:var(--ink);font-family:"Iowan Old Style","Palatino Linotype",Palatino,Georgia,serif;font-size:17px;line-height:1.6}
main{max-width:860px;margin:0 auto;padding:0 1.2rem 5rem}
.mono,label,.themebtn,button,nav.pager,header .tag,header nav,summary time,.note,.warn{font-family:ui-monospace,"Cascadia Mono","SFMono-Regular",Consolas,monospace}
header{padding:2.4rem 0 0}
header .top{display:flex;justify-content:space-between;align-items:flex-end;gap:1rem;flex-wrap:wrap}
header h1{margin:0;font-size:2.6rem;letter-spacing:.32em;text-transform:uppercase;font-weight:700;line-height:1}
header .tag{margin:.7rem 0 0;font-size:.78rem;letter-spacing:.12em;text-transform:uppercase;color:var(--muted)}
header .ps{display:none}
header nav{display:flex;gap:1.4rem;align-items:center;font-size:.8rem;letter-spacing:.1em;text-transform:uppercase}
header nav a,header nav strong{padding:.3rem 0;text-decoration:none;color:var(--muted);border-bottom:2px solid transparent}
header nav a:hover{color:var(--ink)}
header nav strong{color:var(--ink);border-bottom-color:var(--accent)}
.ripple{height:10px;margin:1.3rem 0 1.6rem;background:repeating-linear-gradient(135deg,var(--accent) 0 2px,transparent 2px 7px);border-top:1px solid var(--strong);border-bottom:1px solid var(--strong)}
form.row{display:flex;flex-wrap:wrap;gap:.9rem;align-items:flex-end;border:1px solid var(--strong);background:var(--surface);padding:1rem;margin:0 0 1.4rem}
.cols{display:contents}
label{display:flex;flex-direction:column;gap:.3rem;font-size:.68rem;letter-spacing:.12em;text-transform:uppercase;color:var(--muted)}
select,input[type=text]{font:inherit;font-size:.95rem;color:var(--ink);background:var(--bg);border:1px solid var(--strong);padding:.5rem .7rem}
select{appearance:none;padding-right:1.8rem;background-image:linear-gradient(45deg,transparent 50%,currentColor 50%),linear-gradient(135deg,currentColor 50%,transparent 50%);background-position:calc(100% - 14px) 55%,calc(100% - 9px) 55%;background-size:5px 5px;background-repeat:no-repeat}
input[type=text]{flex:1 1 18rem;min-width:0;font-family:ui-monospace,Consolas,monospace}
select:focus,input:focus{outline:2px solid var(--accent);outline-offset:1px}
button{font-size:.85rem;letter-spacing:.14em;text-transform:uppercase;cursor:pointer;background:var(--accent);color:var(--accent-ink);border:1px solid var(--strong);padding:.62rem 1.4rem;font-weight:700}
button:hover{filter:brightness(1.1)}
.themebtn{background:none;color:var(--muted);border:1px solid var(--line);padding:.25rem .6rem;font-size:.7rem;font-weight:400}
.note,.warn{font-size:.85rem;padding:.7rem 1rem;margin:0 0 .8rem;border:1px solid var(--line);border-left:5px solid var(--link);background:var(--surface)}
.warn{border-left-color:var(--accent)}
details{border:1px solid var(--line);background:var(--surface);margin:0 0 .7rem}
details[open]{border-color:var(--strong)}
summary{cursor:pointer;list-style:none;padding:.7rem 1rem;display:flex;gap:.8rem;align-items:baseline}
summary::-webkit-details-marker{display:none}
summary::before{content:"+";font-family:ui-monospace,monospace;color:var(--accent);font-weight:700;flex:none;width:1ch}
details[open] summary::before{content:"\2212"}
details[open] summary{border-bottom:1px solid var(--line)}
details>div,details>p,details>hr{margin-left:1rem;margin-right:1rem}
details>p{margin-top:.9rem}details>div{padding-bottom:1rem;overflow-wrap:anywhere}
details hr{border:0;border-top:1px dashed var(--line);margin-top:.6rem;margin-bottom:.9rem}
mark{background:var(--mark);color:var(--mark-ink);padding:0 .12em}
nav.pager{display:flex;justify-content:space-between;align-items:center;margin:1.2rem 0;font-size:.82rem;letter-spacing:.06em;color:var(--muted)}
nav.pager a{color:var(--link);text-decoration:none;text-transform:uppercase;letter-spacing:.12em;font-weight:700}
nav.pager a:hover{text-decoration:underline}
code{font-family:ui-monospace,Consolas,monospace;background:var(--bg);border:1px solid var(--line);padding:.05em .35em;font-size:.85em}
a{color:var(--link)}h3{margin-top:1.8rem;letter-spacing:.1em;text-transform:uppercase;font-size:.9rem}
h5{margin:.9rem 0 .3rem;font-family:ui-monospace,monospace;font-size:.78rem;letter-spacing:.1em;text-transform:uppercase;color:var(--accent)}
blockquote{margin:.6rem 0;padding-left:1rem;border-left:3px solid var(--accent);color:var(--muted)}
@media (max-width:620px){header h1{font-size:1.8rem;letter-spacing:.2em}}
/* kali terminal theme */
html[data-theme=kali] body{font-family:"Cascadia Mono","Fira Code",ui-monospace,Consolas,monospace;font-size:14px}
html[data-theme=kali] header .top>div:first-child,html[data-theme=kali] .ripple{display:none}html[data-theme=kali] header nav{margin-bottom:1rem}
html[data-theme=kali] header{padding:1.2rem 0 0}
html[data-theme=kali] header .ps{display:block;white-space:pre;line-height:1.35;margin-bottom:.5rem}
html[data-theme=kali] .ps b{color:#3b8eea;font-weight:400}html[data-theme=kali] .ps i{color:#3fb950;font-style:normal}html[data-theme=kali] .ps u{color:#e6edf3;text-decoration:none}
html[data-theme=kali] header nav{margin-bottom:1rem}
html[data-theme=kali] form.row{border:0;border-left:2px solid var(--line)}
html[data-theme=kali] select,html[data-theme=kali] input[type=text]{border:0;border-bottom:1px solid #3b8eea;background:#0d1117;color:#3fb950}
html[data-theme=kali] button{background:none;color:#3fb950;border-color:#3fb950;font-weight:400}
html[data-theme=kali] button:hover{background:#3fb950;color:#0a0c10;filter:none}
html[data-theme=kali] .note{border-left-color:#3fb950;color:#3fb950}html[data-theme=kali] .warn{color:#e3b341}
html[data-theme=kali] summary::before{content:"[+]";width:auto;color:#3fb950}html[data-theme=kali] details[open] summary::before{content:"[-]"}
html[data-theme=kali] details{border:0;border-left:2px solid var(--line);background:none}
"#;

const SEARCH_GUIDE: &str = "<details><summary>search engine guide</summary><div>\
<h5>keyword search // exact match off</h5>\
<p>finds posts containing query words and ranks them by matching words. enable exact match to require every word or quoted phrase.</p>\
<hr><h5>lexical search // exact match on</h5>\
<p>searches for direct text matches in thread title and post content.</p>\
<ul><li><strong>multiple words (and):</strong> <code>test mast</code> returns posts containing both terms.</li>\
<li><strong>exact phrase:</strong> use quotes, eg., <code>stiff leg</code>, to match the phrase exactly.</li></ul>\
<blockquote><strong>best for:</strong> pinpointing specific dosages, protocols, or literal forum queries.</blockquote>\
<hr><h5>interface controls</h5>\
<ul><li><strong>sort:</strong> relevance, newest, or oldest.</li><li><strong>result scope:</strong> controls retrieval depth (top 100 vs all matches).</li>\
<li><strong>page size:</strong> controls how many entries render per page.</li></ul></div></details>";

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"en\" data-theme=\"dune\"><head><script>try{{var t=localStorage.getItem('theme');if(t=='kali'||t=='dune')document.documentElement.dataset.theme=t}}catch(e){{}}</script><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><style>{STYLE}</style></head><body>{body}</main><script>document.querySelectorAll('.themebtn').forEach(function(b){{b.onclick=function(){{var d=document.documentElement,n=d.dataset.theme=='kali'?'dune':'kali';d.dataset.theme=n;try{{localStorage.setItem('theme',n)}}catch(e){{}}}}}})</script></body></html>",
        escape(title)
    ))
}

fn nav(active: &str) -> String {
    let link = |href: &str, label: &str| {
        if label == active { format!("<strong>{label}</strong>") } else { format!("<a href=\"{href}\">{label}</a>") }
    };
    let (search, rag) = (link("/", "search"), link("/rag", "rag mode *experimental"));
    format!(
        "<main><header><span class=\"ps\"><b>\u{250C}\u{2500}\u{2500}(</b><i>archive\u{327F}jordan</i><b>)-[</b><u>~/forum</u><b>]</b>\n<b>\u{2514}\u{2500}</b><i>$</i> <u>{}</u></span>\
         <div class=\"top\"><div><h1>archive</h1><p class=\"tag\">jordan peters\u{2019} forum archive</p></div>\
         <nav>{search}{rag}<button type=\"button\" class=\"themebtn\">theme</button></nav></div><div class=\"ripple\"></div></header>",
        escape(active),
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
    body.push_str("<p class=\"note\">directly search the raw logbook archives. no ai synthesis.</p>");

    body.push_str(&format!(
        "<form class=\"row\" method=\"get\" action=\"/\"><div class=\"cols\">{}{}{}{}</div>\
         <input type=\"text\" name=\"q\" value=\"{}\" placeholder=\"use quotes for exact phrase: &quot;trap bar&quot; rdl\">\
         <button type=\"submit\">search</button></form>",
        select("scope", "result scope", &["all matches", "top 50", "top 100", "top 200", "top 500"], &scope),
        select("sort", "sort", &["relevance", "newest", "oldest"], &sort),
        select("size", "page size", &["10", "20", "50", "100"], &size.to_string()),
        select("exact", "exact match", &["off", "on"], if exact { "on" } else { "off" }),
        escape(&query),
    ));

    body.push_str(SEARCH_GUIDE);

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
        if page_index > 0 { link(page_index - 1, "previous") } else { "<span></span>".to_string() },
        start + 1,
        end,
        total,
        page_index + 1,
        total_pages,
        if page_index + 1 < total_pages { link(page_index + 1, "next") } else { "<span></span>".to_string() },
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
