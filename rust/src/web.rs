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
html[data-theme=akita]{--bg:#1b1508;--surface:#191a1c;--ink:#cfcabb;--muted:#8d8878;--line:#2e2d29;--bar:#3a3322;--accent:#e2614b;--accent-ink:#1b1508;--link:#e2614b;--mark:#6b3a12;--mark-ink:#ffe9c9;--chip:#121212}
@media (prefers-color-scheme:light){html[data-theme=akita]{--bg:#f4eee0;--surface:#fbf7ec;--ink:#2b2518;--muted:#7a7160;--line:#d9cfb6;--bar:#cfc3a3;--accent:#b8412b;--accent-ink:#fbf7ec;--link:#b8412b;--mark:#f3c978;--mark-ink:#2b2518;--chip:#f0e8d2}}
html[data-theme=kali]{--bg:#0a0c10;--surface:#0a0c10;--ink:#c9d1d9;--muted:#6e7681;--line:#21262d;--bar:#21262d;--accent:#3b8eea;--accent-ink:#0a0c10;--link:#3b8eea;--mark:#e3b341;--mark-ink:#0a0c10;--chip:#0d1117}
:root{--serif:"Spectral","Source Serif 4","Iowan Old Style","Palatino Linotype",Georgia,serif;--sans:Inter,"Segoe UI",system-ui,-apple-system,sans-serif;--mono:"JetBrains Mono","Cascadia Mono",ui-monospace,Consolas,monospace}
body{margin:0;background:var(--bg);color:var(--ink);font-family:var(--sans);font-size:15px;line-height:1.6}
.topbar{border-bottom:1px solid var(--bar)}
.bar{max-width:1120px;margin:0 auto;padding:.7rem 1.2rem;display:flex;align-items:center;gap:1.6rem}
.brand{font-family:var(--serif);font-size:1.45rem;color:var(--ink);letter-spacing:.01em}
.bar nav{display:flex;gap:1.3rem;flex:1;font-family:var(--mono);font-size:.78rem}
.bar nav a,.bar nav strong{padding:.5rem 0;color:var(--muted);text-decoration:none;border-bottom:2px solid transparent;font-weight:500}
.bar nav a:hover{color:var(--ink)}
.bar nav strong{color:var(--ink);border-bottom-color:var(--muted)}
.themebtn{font-family:var(--mono);font-size:.72rem;background:none;color:var(--muted);border:1px solid var(--line);padding:.35rem .7rem;cursor:pointer}
.themebtn:hover{color:var(--ink);border-color:var(--muted)}
.sub{border-bottom:1px solid var(--line);font-size:.8rem;color:var(--muted)}
.sub>span{display:inline-block}.sub{padding:.55rem max(1.2rem,calc((100% - 1120px)/2 + 1.2rem))}.sub .dot{margin:0 .6rem}
main{max-width:1120px;margin:0 auto;padding:0 1.2rem 5rem}
header .ps{display:none}
h2.sec{font-family:var(--serif);font-weight:500;font-size:2rem;margin:2.2rem 0 .8rem;color:var(--ink);line-height:1.2}
.rule{border:0;border-top:1px solid var(--line);margin:0 0 1.4rem}
form.row{display:flex;flex-wrap:wrap;gap:.9rem;align-items:flex-end;border:1px solid var(--line);background:var(--surface);padding:1rem;margin:0 0 1.2rem}
.cols{display:contents}
label{display:flex;flex-direction:column;gap:.3rem;font-family:var(--mono);font-size:.68rem;text-transform:uppercase;letter-spacing:.08em;color:var(--muted)}
select,input[type=text]{font:inherit;font-size:.92rem;color:var(--ink);background:var(--bg);border:1px solid var(--line);padding:.5rem .7rem}
select{appearance:none;padding-right:1.8rem;background-image:linear-gradient(45deg,transparent 50%,currentColor 50%),linear-gradient(135deg,currentColor 50%,transparent 50%);background-position:calc(100% - 14px) 55%,calc(100% - 9px) 55%;background-size:5px 5px;background-repeat:no-repeat}
input[type=text]{flex:1 1 18rem;min-width:0;font-family:var(--mono);font-size:.88rem}
select:focus,input:focus{outline:1px solid var(--accent);outline-offset:0;border-color:var(--accent)}
button[type=submit]{font-family:var(--mono);font-size:.8rem;letter-spacing:.06em;cursor:pointer;background:var(--accent);color:var(--accent-ink);border:1px solid var(--accent);padding:.6rem 1.3rem;font-weight:700}
button[type=submit]:hover{filter:brightness(1.1)}
.note,.warn{font-size:.84rem;padding:.65rem 1rem;margin:0 0 .7rem;border:1px solid var(--line);border-left:3px solid var(--muted);background:var(--surface);color:var(--muted)}
.warn{border-left-color:var(--accent);color:var(--ink)}
.guide{border:1px solid var(--line);background:var(--surface);margin:0 0 .5rem}
.guide>summary{cursor:pointer;list-style:none;padding:.65rem 1rem;font-family:var(--mono);font-size:.8rem;color:var(--muted)}
.guide>summary::-webkit-details-marker{display:none}
.guide>summary::before{content:"+ ";color:var(--accent)}.guide[open]>summary::before{content:"\2212 "}
.guide>div{padding:0 1rem 1rem}
.pager{display:flex;align-items:center;gap:1rem;margin:2rem 0 .8rem;padding-bottom:.6rem;border-bottom:1px solid var(--line)}
.pager h2.sec{margin:0;flex:1}
.pager .count{font-family:var(--mono);font-size:.78rem;color:var(--muted)}
.pg{font-family:var(--mono);display:inline-flex;align-items:center;justify-content:center;width:2.2rem;height:2.2rem;border:1px solid var(--line);background:var(--surface);color:var(--ink);text-decoration:none;font-size:1rem}
a.pg:hover{border-color:var(--accent);color:var(--accent)}.pg.off{color:var(--line)}
.pgs{display:flex;gap:.4rem}
.month{display:flex;align-items:center;gap:.8rem;margin:1.6rem 0 .6rem;font-family:var(--mono);font-size:.72rem;letter-spacing:.08em;text-transform:uppercase;color:var(--muted)}
.month i{flex:1;border-top:1px solid var(--bar)}
details.row{border:1px solid var(--line);background:var(--surface);margin:0 0 -1px}
details.row[open]{border-color:var(--muted);position:relative}
details.row>summary{cursor:pointer;list-style:none;display:grid;grid-template-columns:6.2rem 1fr auto;gap:1.2rem;padding:.95rem 1.1rem;align-items:start}
details.row>summary::-webkit-details-marker{display:none}
details.row>summary:hover .title{text-decoration:underline}
summary time{font-family:var(--mono);font-size:.74rem;color:var(--accent);padding-top:.28rem}
.title{display:block;font-family:var(--serif);font-size:1.15rem;line-height:1.3;color:var(--link)}
.excerpt{display:block;margin-top:.25rem;font-size:.84rem;color:var(--muted);overflow:hidden;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical}
details[open] .excerpt{display:none}
.chips{display:flex;gap:.4rem;flex-wrap:wrap;justify-content:flex-end;max-width:14rem}
.chip{font-family:var(--mono);font-size:.7rem;color:var(--ink);background:var(--chip);border:1px solid var(--bar);padding:.12rem .45rem;white-space:nowrap}
.full{padding:0 1.1rem 1.1rem;border-top:1px solid var(--line)}
.full p{margin:.9rem 0 0}.full hr{border:0;border-top:1px dashed var(--line);margin:.8rem 0}
.full div{overflow-wrap:anywhere}
mark{background:var(--mark);color:var(--mark-ink);padding:0 .1em}
code{font-family:var(--mono);background:var(--bg);border:1px solid var(--line);padding:.05em .35em;font-size:.85em}
a{color:var(--link)}
h5{margin:.9rem 0 .3rem;font-family:var(--mono);font-size:.74rem;letter-spacing:.08em;text-transform:uppercase;color:var(--accent)}
blockquote{margin:.6rem 0;padding-left:1rem;border-left:3px solid var(--accent);color:var(--muted)}
h3{font-family:var(--serif);font-weight:500;font-size:1.3rem}
@media (max-width:720px){details.row>summary{grid-template-columns:1fr}.chips{justify-content:flex-start;max-width:none}.bar{flex-wrap:wrap}}
/* kali terminal theme */
html[data-theme=kali] body{font-family:"Cascadia Mono","Fira Code",ui-monospace,Consolas,monospace;font-size:14px}
html[data-theme=kali] .brand,html[data-theme=kali] .sub,html[data-theme=kali] h2.sec,html[data-theme=kali] .rule{display:none}
html[data-theme=kali] .topbar{border:0}
html[data-theme=kali] .bar{padding:.4rem 1.2rem}
html[data-theme=kali] header .ps{display:block;white-space:pre;line-height:1.35;padding-top:1rem;margin-bottom:.5rem}
html[data-theme=kali] .ps b{color:#3b8eea;font-weight:400}html[data-theme=kali] .ps i{color:#3fb950;font-style:normal}html[data-theme=kali] .ps u{color:#e6edf3;text-decoration:none}
html[data-theme=kali] form.row{border:0;border-left:2px solid var(--line)}
html[data-theme=kali] select,html[data-theme=kali] input[type=text]{border:0;border-bottom:1px solid #3b8eea;background:#0d1117;color:#3fb950}
html[data-theme=kali] button[type=submit]{background:none;color:#3fb950;border-color:#3fb950;font-weight:400}
html[data-theme=kali] .note{border-left-color:#3fb950;color:#3fb950}
html[data-theme=kali] details.row,html[data-theme=kali] .guide{border:0;border-left:2px solid var(--line);background:none;margin:0 0 .5rem}
html[data-theme=kali] details.row>summary{grid-template-columns:auto 1fr auto}
html[data-theme=kali] .title{font-family:inherit;font-size:1em;color:var(--ink)}
html[data-theme=kali] summary time{color:#3fb950}
html[data-theme=kali] .excerpt,html[data-theme=kali] .chips{display:none}
html[data-theme=kali] .pager{border:0}html[data-theme=kali] .pager .count{margin-left:auto}
"#;

const SEARCH_GUIDE: &str = "<details class=\"guide\"><summary>search engine guide</summary><div>\
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
        "<!doctype html><html lang=\"en\" data-theme=\"akita\"><head><script>try{{var t=localStorage.getItem('theme');if(t=='kali'||t=='akita')document.documentElement.dataset.theme=t}}catch(e){{}}</script><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><style>{STYLE}</style></head><body>{body}</main><script>document.querySelectorAll('.themebtn').forEach(function(b){{b.onclick=function(){{var d=document.documentElement,n=d.dataset.theme=='kali'?'akita':'kali';d.dataset.theme=n;try{{localStorage.setItem('theme',n)}}catch(e){{}}}}}})</script></body></html>",
        escape(title)
    ))
}

fn nav(active: &str) -> String {
    let link = |href: &str, label: &str| {
        if label == active { format!("<strong>{label}</strong>") } else { format!("<a href=\"{href}\">{label}</a>") }
    };
    format!(
        "<div class=\"topbar\"><div class=\"bar\"><span class=\"brand\">archive</span>\
         <nav>{}{}</nav><button type=\"button\" class=\"themebtn\">theme</button></div></div>\
         <div class=\"sub\"><span>jordan peters\u{2019} forum archive</span><span class=\"dot\">\u{b7}</span><span>keyword search, no ai synthesis</span></div>\
         <main><header><span class=\"ps\"><b>\u{250C}\u{2500}\u{2500}(</b><i>archive\u{327F}jordan</i><b>)-[</b><u>~/forum</u><b>]</b>\n<b>\u{2514}\u{2500}</b><i>$</i> <u>{}</u></span></header>",
        link("/", "search"),
        link("/rag", "rag mode *experimental"),
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

/// "May 10, 2025 at 6:20 am" -> "May 10, 2025".
fn short_date(date: &str) -> &str {
    date.split(" at ").next().unwrap_or(date)
}

/// "May 10, 2025" -> "MAY 2025" (used for the month headings).
fn month_key(date: &str) -> String {
    let d = short_date(date);
    let month = d.split_whitespace().next().unwrap_or("");
    let year = d.rsplit(", ").next().unwrap_or("");
    format!("{month} {year}").to_uppercase()
}

/// "October 20, 2024" -> "Oct 20, 2024".
fn abbreviate_month(date: &str) -> String {
    match date.split_once(' ') {
        Some((month, rest)) if month.chars().count() > 3 => {
            format!("{} {rest}", month.chars().take(3).collect::<String>())
        }
        _ => date.to_string(),
    }
}

fn render_document(doc: &Chunk, query: &str) -> String {
    let m = &doc.metadata;
    let id = m.post_id.as_deref().unwrap_or("unknown");
    let date = m.date.as_deref().unwrap_or("unknown date");
    let title = m.thread_title.as_deref().unwrap_or("unknown thread");
    let flat: String = doc.page_content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut excerpt: String = flat.chars().take(180).collect();
    if flat.chars().count() > 180 {
        excerpt.push('\u{2026}');
    }
    let year = short_date(date).rsplit(", ").next().unwrap_or("");
    format!(
        "<details class=\"row\"><summary><time>{}</time><span class=\"main\"><span class=\"title\">{}</span>\
         <span class=\"excerpt\">{}</span></span><span class=\"chips\"><span class=\"chip\">#{}</span>\
         <span class=\"chip\">#{}</span></span></summary>\
         <div class=\"full\"><p><strong>{}</strong></p><hr><div>{}</div></div></details>",
        escape(&abbreviate_month(short_date(date))),
        highlight_text(title, query).replace("<br>", ""),
        highlight_text(&excerpt, query),
        escape(id),
        escape(year),
        escape(date),
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
    body.push_str("<h2 class=\"sec\">search the archive</h2><hr class=\"rule\">");
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
    let prev = if page_index > 0 { link(page_index - 1, "\u{2039}").replace("<a ", "<a class=\"pg\" ") } else { "<span class=\"pg off\">\u{2039}</span>".to_string() };
    let next = if page_index + 1 < total_pages { link(page_index + 1, "\u{203A}").replace("<a ", "<a class=\"pg\" ") } else { "<span class=\"pg off\">\u{203A}</span>".to_string() };
    body.push_str(&format!(
        "<div class=\"pager\"><h2 class=\"sec\">results</h2><span class=\"count\">{}\u{2013}{} of {} \u{b7} page {}/{}</span><span class=\"pgs\">{prev}{next}</span></div>",
        start + 1,
        end,
        total,
        page_index + 1,
        total_pages,
    ));
    // Newest/oldest sorts read like a timeline, so group them under month headings.
    let group = sort != "relevance";
    let mut last_month = String::new();
    for doc in &results[start..end] {
        if group {
            let key = month_key(doc.metadata.date.as_deref().unwrap_or(""));
            if key != last_month {
                let n = results[start..end]
                    .iter()
                    .filter(|d| month_key(d.metadata.date.as_deref().unwrap_or("")) == key)
                    .count();
                body.push_str(&format!(
                    "<div class=\"month\"><span>{}</span><i></i><span>{n} {}</span></div>",
                    escape(&key),
                    if n == 1 { "entry" } else { "entries" }
                ));
                last_month = key;
            }
        }
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
    body.push_str("<h2 class=\"sec\">ask the archive</h2><hr class=\"rule\">");
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
