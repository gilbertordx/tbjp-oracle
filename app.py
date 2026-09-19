from dotenv import load_dotenv
import html
import math
import re

import streamlit as st

from src.vector_store import TBJPVectorStore

load_dotenv()

st.set_page_config(page_title="archive", page_icon="📚", layout="centered")


@st.cache_resource
def init_system():
    db = TBJPVectorStore()
    return db


def parse_query_tokens(query: str) -> tuple[list[str], list[str]]:
    phrases: list[str] = []
    terms: list[str] = []

    for phrase in re.findall(r'"([^"]+)"', query):
        cleaned = phrase.strip()
        if cleaned:
            phrases.append(cleaned)

    remainder = re.sub(r'"[^"]+"', " ", query)
    for token in re.findall(r"[a-z0-9]+", remainder.lower()):
        if token == "and":
            continue
        terms.append(token)

    return phrases, terms


def highlight_text(text: str, query: str) -> str:
    if not query.strip():
        return html.escape(text).replace("\n", "<br>")

    phrases, terms = parse_query_tokens(query)
    escaped_text = html.escape(text)

    pattern_parts: list[str] = []
    for phrase in phrases:
        pattern_parts.append(re.escape(phrase))
    for term in terms:
        pattern_parts.append(rf"\b{re.escape(term)}\b")

    if not pattern_parts:
        return escaped_text.replace("\n", "<br>")

    pattern_parts.sort(key=len, reverse=True)
    combined_pattern = re.compile("(" + "|".join(pattern_parts) + ")", flags=re.IGNORECASE)
    highlighted = combined_pattern.sub(r"<mark>\1</mark>", escaped_text)
    return highlighted.replace("\n", "<br>")


def render_source_document(doc, highlight_query: str = "", show_send_button: bool = True):
    """standardized renderer for a logbook entry."""
    post_id = doc.metadata.get("post_id", "unknown id")
    post_date = doc.metadata.get("date", "unknown date")
    thread_title = doc.metadata.get("thread_title", doc.metadata.get("title", "unknown thread"))

    with st.expander(f"{post_date} | {thread_title} (id: {post_id})"):
        if highlight_query.strip():
            highlighted_title = highlight_text(thread_title, highlight_query).replace("<br>", "")
            st.markdown(f"**thread:** {highlighted_title}", unsafe_allow_html=True)
            st.markdown("---")
            st.markdown(highlight_text(doc.page_content, highlight_query), unsafe_allow_html=True)
        else:
            st.markdown(f"**thread:** {thread_title}")
            st.markdown("---")
            st.markdown(doc.page_content)

        if show_send_button:
            st.markdown("---")
            st.button(
                "rag mode *experimental", key=f"analyze_{post_id}", disabled=True,
                help="experimental feature. currently unavailable until a replacement model is configured.",
            )


st.title("archive")
st.markdown("search jordan peters’ forum archive.")

try:
    db = init_system()
except Exception as e:
    st.error(f"initialization failed: {e}")
    st.stop()

if "consultant_results" not in st.session_state:
    st.session_state.consultant_results = []
if "page_index" not in st.session_state:
    st.session_state.page_index = 0
if "consultant_signature" not in st.session_state:
    st.session_state.consultant_signature = None
if "consultant_active_query" not in st.session_state:
    st.session_state.consultant_active_query = ""
if st.session_state.get("app_mode") not in ("search", "rag mode *experimental"):
    previous_mode = st.session_state.get("app_mode", "search")
    st.session_state.app_mode = "search" if previous_mode.lower() == "search" else "rag mode *experimental"

if db.status_message:
    st.info("keyword search is active. semantic search is not configured on this installation.")

mode = st.sidebar.radio("select mode", ["search", "rag mode *experimental"], key="app_mode")

if mode == "search":
    st.info("directly search the raw logbook archives. no ai synthesis.")

    with st.expander("search engine guide", expanded=False):
        if db.backend == "chroma":
            st.markdown("##### semantic search // exact match off")
            st.markdown("searches by *concept* and meaning. if you search `insulin`, results can include posts about carbohydrates, diet, and blood sugar even when the exact word is absent.")
            st.markdown("> **best for:** broad research and discovering related principles.")
        else:
            st.markdown("##### keyword search // exact match off")
            st.markdown("finds posts containing query words and ranks them by matching words. enable exact match to require every word or quoted phrase.")

        st.markdown("---")

        st.markdown("##### lexical search // exact match on ")
        st.markdown("searches for direct text matches in thread title and post content.")
        st.markdown("- **multiple words (and):** `test mast` returns posts containing both terms.\n- **exact phrase:** use quotes, eg., `stiff leg`, to match the phrase exactly.")
        st.markdown("> **best for:** pinpointing specific dosages, protocols, or literal forum queries.")

        st.markdown("---")

        st.markdown("##### interface controls")
        st.markdown("- **sort:** relevance, newest, or oldest.\n- **result scope:** controls retrieval depth (top 100 vs all matches).\n- **page size:** controls how many entries render per page.")

    col_scope, col_sort, col_page, col_exact = st.columns([1, 1, 1, 1])
    with col_scope:
        result_scope = st.selectbox(
            "result scope",
            options=["all matches", "top 50", "top 100", "top 200", "top 500"],
            index=0,
        )
    with col_sort:
        sort_by = st.selectbox("sort", options=["relevance", "newest", "oldest"], index=0)
    with col_page:
        page_size = st.selectbox("page size", options=[10, 20, 50, 100], index=1)
    with col_exact:
        exact_match_val = st.selectbox("exact match", options=["off", "on"], index=0)
        exact_match = exact_match_val == "on"

    search_query = st.text_input(
        "search the logbook",
        placeholder="use quotes for exact phrase: \"trap bar\" rdl",
    )

    search_clicked = st.button("search", type="primary")

    scope_to_k = {
        "all matches": 0,
        "top 50": 50,
        "top 100": 100,
        "top 200": 200,
        "top 500": 500,
    }
    search_signature = (search_query.strip(), result_scope, sort_by, exact_match)

    if search_clicked:
        if not search_query.strip():
            st.warning("enter a search query first.")
        else:
            with st.spinner(f"searching for '{search_query}'..."):
                try:
                    k_value = scope_to_k[result_scope]
                    results = db.hybrid_search(
                        query=search_query,
                        k=k_value,
                        exact_match=exact_match,
                        sort_by=sort_by.lower(),
                    )
                    st.session_state.consultant_results = results
                    st.session_state.page_index = 0
                    st.session_state.consultant_signature = search_signature
                    st.session_state.consultant_active_query = search_query
                except Exception as e:
                    st.error(f"search failed: {e}")

    if st.session_state.consultant_signature and search_signature != st.session_state.consultant_signature:
        st.caption("filters changed. click search to refresh results.")

    total_results = len(st.session_state.consultant_results)
    if total_results > 0:
        total_pages = max(1, math.ceil(total_results / page_size))
        st.session_state.page_index = min(st.session_state.page_index, total_pages - 1)

        start_index = st.session_state.page_index * page_size
        end_index = min(start_index + page_size, total_results)

        nav_prev, nav_meta, nav_next = st.columns([1, 2, 1])
        with nav_prev:
            if st.button("previous", disabled=st.session_state.page_index <= 0):
                st.session_state.page_index -= 1
                st.rerun()
        with nav_meta:
            st.markdown(
                f"**showing {start_index + 1}-{end_index} of {total_results}**  "
                + f"(page {st.session_state.page_index + 1}/{total_pages})"
            )
        with nav_next:
            if st.button("next", disabled=st.session_state.page_index >= total_pages - 1):
                st.session_state.page_index += 1
                st.rerun()

        for doc in st.session_state.consultant_results[start_index:end_index]:
            render_source_document(doc, highlight_query=st.session_state.consultant_active_query)
    elif st.session_state.consultant_signature:
        st.warning("no matching entries found for this query.")

elif mode == "rag mode *experimental":
    st.subheader("rag mode *experimental")
    st.warning("experimental feature: ai answers may be incomplete or inaccurate. verify claims against the original forum posts.")
    st.info("currently unavailable while we choose a free replacement model. archive search is available without an api key.")
