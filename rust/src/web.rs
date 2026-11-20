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
*{box-sizing:border-box}
html[data-theme=streamlit]{--bg:#fff;--sec:#f0f2f6;--ink:#31333f;--muted:rgba(49,51,63,.6);--line:rgba(49,51,63,.2);--accent:#ff4b4b;--info-bg:rgba(28,131,225,.1);--info-ink:#004280;--warn-bg:rgba(255,227,18,.1);--warn-ink:#926c05;--mark:#fff3a3}
@media (prefers-color-scheme:dark){html[data-theme=streamlit]{--bg:#0e1117;--sec:#262730;--ink:#fafafa;--muted:rgba(250,250,250,.6);--line:rgba(250,250,250,.2);--info-bg:rgba(61,157,243,.2);--info-ink:#c7ebff;--warn-bg:rgba(255,227,18,.2);--warn-ink:#ffffc2;--mark:#6b5300}}
html[data-theme=kali]{--bg:#0a0c10;--sec:#0a0c10;--ink:#c9d1d9;--muted:#6e7681;--line:#21262d;--accent:#3b8eea;--info-bg:#0d1f12;--info-ink:#3fb950;--warn-bg:#2a1a0a;--warn-ink:#e3b341;--mark:#e3b341}
body{margin:0;background:var(--bg);color:var(--ink);font-family:"Source Sans Pro","Source Sans 3",-apple-system,"Segoe UI",Roboto,sans-serif;font-size:16px;line-height:1.6}
.app{display:flex;min-height:100vh}
aside.sb{width:244px;flex:none;background:var(--sec);padding:2rem 1.2rem;position:sticky;top:0;height:100vh;overflow:auto}
aside.sb h4{margin:0 0 .6rem;font-size:.875rem;font-weight:400;color:var(--muted)}
aside.sb a,aside.sb strong{display:flex;align-items:flex-start;gap:.5rem;padding:.25rem 0;color:var(--ink);text-decoration:none;font-weight:400}
aside.sb a::before,aside.sb strong::before{content:"";margin-top:.3rem;width:1rem;height:1rem;border-radius:50%;border:1px solid var(--line);flex:none}
aside.sb strong::before{border:5px solid var(--accent);background:#fff}
aside.sb .themebtn{margin-top:2rem}
main{flex:1;min-width:0;max-width:730px;margin:0 auto;padding:4rem 1.2rem 6rem}
header h1{font-size:2.75rem;font-weight:700;margin:0;padding:1.25rem 0 1rem;letter-spacing:-.005em;line-height:1.2}
header h1 span{color:inherit}
header p{margin:0 0 1rem}
header nav{display:none}header .ps{display:none}
.cols{display:grid;grid-template-columns:repeat(4,1fr);gap:1rem}
form.row{display:block;margin:1rem 0}
form.row .cols+input{margin-top:1rem}
label{display:flex;flex-direction:column;font-size:.875rem;color:var(--ink);gap:.25rem}
select,input[type=text]{font:inherit;color:var(--ink);background:var(--sec);border:1px solid transparent;border-radius:.5rem;padding:.5rem .75rem;width:100%}
select:focus,input:focus{outline:0;border-color:var(--accent)}
input[type=text]{display:block;margin-bottom:1rem}
button,.themebtn{font:inherit;background:var(--bg);color:var(--ink);border:1px solid var(--line);border-radius:.5rem;padding:.4rem 1rem;cursor:pointer}
button[type=submit]{background:var(--accent);border-color:var(--accent);color:#fff}
button:hover{border-color:var(--accent);color:var(--accent)}button[type=submit]:hover{color:#fff;filter:brightness(1.08)}
details{border:1px solid var(--line);border-radius:.5rem;margin:0 0 1rem;padding:.6rem 1rem}
summary{cursor:pointer;list-style:none;display:flex;justify-content:space-between;gap:1rem}
summary::-webkit-details-marker{display:none}
summary::after{content:"\2335";font-size:.8rem;color:var(--muted);flex:none}details[open] summary::after{transform:rotate(180deg)}
details hr{border:0;border-top:1px solid var(--line);margin:1rem 0}
details div{overflow-wrap:anywhere}
mark{background:var(--mark);color:inherit;border-radius:3px;padding:0 .1em}
nav.pager{display:flex;gap:1rem;align-items:center;justify-content:space-between;margin:1rem 0}
nav.pager a{border:1px solid var(--line);border-radius:.5rem;padding:.3rem .9rem;color:var(--ink);text-decoration:none}
nav.pager a:hover{border-color:var(--accent);color:var(--accent)}
nav.pager>span:first-child:empty{width:5rem}
.note,.warn{padding:1rem;border-radius:.5rem;margin:0 0 1rem}
.note{background:var(--info-bg);color:var(--info-ink)}.warn{background:var(--warn-bg);color:var(--warn-ink)}
code{background:var(--sec);padding:.1em .35em;border-radius:4px;font-size:.875em}
a{color:var(--accent)}h3{margin-top:1.6rem}
blockquote{margin:.5rem 0;padding-left:1rem;border-left:3px solid var(--line);color:var(--muted)}
@media (max-width:820px){aside.sb{display:none}html[data-theme=streamlit] header nav{display:flex;gap:.6rem;margin-bottom:1rem}header nav a,header nav strong{padding:.2rem .7rem;border:1px solid var(--line);border-radius:999px;font-weight:400;text-decoration:none;color:var(--ink)}.cols{grid-template-columns:1fr 1fr}}
/* kali terminal theme */
html[data-theme=kali] aside.sb{display:none}
html[data-theme=kali] main{max-width:980px;padding:1rem 1rem 4rem}
html[data-theme=kali] body{font-family:"Cascadia Mono","Fira Code",ui-monospace,Consolas,monospace;font-size:14px}
html[data-theme=kali] header h1,html[data-theme=kali] header p{display:none}
html[data-theme=kali] header .ps{display:block;white-space:pre;line-height:1.35}
html[data-theme=kali] .ps b{color:#3b8eea;font-weight:400}html[data-theme=kali] .ps i{color:#3fb950;font-style:normal}html[data-theme=kali] .ps u{color:#e6edf3;text-decoration:none}
html[data-theme=kali] header nav{display:flex;gap:.5rem;margin:.5rem 0 1rem}
html[data-theme=kali] header nav a,html[data-theme=kali] header nav strong,html[data-theme=kali] .themebtn{border-radius:0;font-family:inherit;padding:.1rem .5rem;border:1px solid var(--line);background:none;color:var(--muted);text-decoration:none;font-weight:400}
html[data-theme=kali] header nav strong{color:#3fb950;border-color:#3fb950}
html[data-theme=kali] form.row{display:flex;gap:.6rem;flex-wrap:wrap;align-items:flex-end;border-left:2px solid var(--line);padding:.9rem}
html[data-theme=kali] .cols{display:contents}
html[data-theme=kali] details{border:0;border-left:2px solid var(--line);border-radius:0}
html[data-theme=kali] details[open]{border-left-color:#3b8eea}
html[data-theme=kali] select,html[data-theme=kali] input[type=text]{border-radius:0;border:0;border-bottom:1px solid #3b8eea;background:#0d1117;color:#3fb950;font-family:inherit;width:auto}
html[data-theme=kali] input[type=text]{flex:1;min-width:16rem;margin:0}
html[data-theme=kali] label{color:#6e7681;text-transform:lowercase}
html[data-theme=kali] button{border-radius:0;background:none;color:#3fb950;border:1px solid #3fb950;font-family:inherit}
html[data-theme=kali] button:hover{background:#3fb950;color:#0a0c10}
html[data-theme=kali] .note,html[data-theme=kali] .warn{border-radius:0;border-left:2px solid currentColor}
html[data-theme=kali] mark{background:#e3b341;color:#0a0c10;border-radius:0}
html[data-theme=kali] summary{justify-content:flex-start}
html[data-theme=kali] summary::after{content:none}
html[data-theme=kali] summary::before{content:"[+] ";color:#3fb950;white-space:pre}html[data-theme=kali] details[open] summary::before{content:"[-] "}
html[data-theme=kali] nav.pager a{border:0;padding:0;color:#3b8eea;font-weight:700}
html[data-theme=kali] code{background:#161b22;color:#3fb950}
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
        "<!doctype html><html lang=\"en\" data-theme=\"streamlit\"><head><script>try{{var t=localStorage.getItem('theme');if(t=='kali'||t=='streamlit')document.documentElement.dataset.theme=t}}catch(e){{}}</script><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><style>{STYLE}</style></head><body>{body}</main></div><script>document.querySelectorAll('.themebtn').forEach(function(b){{b.onclick=function(){{var d=document.documentElement,n=d.dataset.theme=='kali'?'streamlit':'kali';d.dataset.theme=n;try{{localStorage.setItem('theme',n)}}catch(e){{}}}}}})</script></body></html>",
        escape(title)
    ))
}

fn nav(active: &str) -> String {
    let link = |href: &str, label: &str| {
        if label == active { format!("<strong>{label}</strong>") } else { format!("<a href=\"{href}\">{label}</a>") }
    };
    let (search, rag) = (link("/", "search"), link("/rag", "rag mode *experimental"));
    format!(
        "<div class=\"app\"><aside class=\"sb\"><h4>select mode</h4>{search}{rag}\
         <button type=\"button\" class=\"themebtn\">theme</button></aside><main>\
         <header><h1>archive</h1><p>search jordan peters\u{2019} forum archive.</p>\
         <span class=\"ps\"><b>\u{250C}\u{2500}\u{2500}(</b><i>archive\u{327F}jordan</i><b>)-[</b><u>~/forum</u><b>]</b>\n<b>\u{2514}\u{2500}</b><i>$</i> <u>{}</u></span>\
         <nav>{search} {rag} <button type=\"button\" class=\"themebtn\">theme</button></nav></header>",
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
    body.push_str(SEARCH_GUIDE);

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
