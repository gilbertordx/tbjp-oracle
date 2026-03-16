from dotenv import load_dotenv
import html
import math
import re

import streamlit as st

from src.llm_engine import TBJPOracleLLM
from src.vector_store import TBJPVectorStore

load_dotenv()

st.set_page_config(page_title="TBJP Oracle", page_icon="||||-----||||", layout="centered")


@st.cache_resource
def init_system():
    db = TBJPVectorStore()
    retriever = db.get_retriever(target_results=5)
    qa_chain = TBJPOracleLLM().build_qa_chain(retriever)
    return db, qa_chain


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
    """Standardized renderer for a logbook entry."""
    post_id = doc.metadata.get("post_id", "Unknown ID")
    post_date = doc.metadata.get("date", "Unknown Date")
    thread_title = doc.metadata.get("thread_title", doc.metadata.get("title", "Unknown Thread"))

    with st.expander(f"{post_date} | {thread_title} (ID: {post_id})"):
        if highlight_query.strip():
            highlighted_title = highlight_text(thread_title, highlight_query).replace("<br>", "")
            st.markdown(f"**Thread:** {highlighted_title}", unsafe_allow_html=True)
            st.markdown("---")
            st.markdown(highlight_text(doc.page_content, highlight_query), unsafe_allow_html=True)
        else:
            st.markdown(f"**Thread:** {thread_title}")
            st.markdown("---")
            st.markdown(doc.page_content)

        if show_send_button:
            st.markdown("---")
            if st.button("Send to Oracle for Analysis", key=f"oracle_{post_id}"):
                st.session_state.app_mode = "Oracle //Chat"
                st.session_state.focus_doc = doc
                st.rerun()


st.title("TBJP Oracle")
st.markdown("### Structural Bodybuilding & Coaching RAG POC")

try:
    db, qa_chain = init_system()
except Exception as e:
    st.error(f"Initialization failed: {e}")
    st.stop()

if "messages" not in st.session_state:
    st.session_state.messages = []
if "consultant_results" not in st.session_state:
    st.session_state.consultant_results = []
if "page_index" not in st.session_state:
    st.session_state.page_index = 0
if "consultant_signature" not in st.session_state:
    st.session_state.consultant_signature = None
if "consultant_active_query" not in st.session_state:
    st.session_state.consultant_active_query = ""
if "focus_doc" not in st.session_state:
    st.session_state.focus_doc = None
if "app_mode" not in st.session_state:
    st.session_state.app_mode = "Consultant //Wiki"

mode = st.sidebar.radio("Select Mode", ["Consultant //Wiki", "Oracle //Chat"], key="app_mode")

if mode == "Consultant //Wiki":
    st.info("Directly search the raw logbook archives. No AI synthesis.")

    with st.expander("Search Engine Guide", expanded=False):
        st.markdown("##### SEMANTIC SEARCH // Exact Match OFF")
        st.markdown("Searches by *concept* and meaning. If you search `INSULIN`, results can include posts about carbohydrates, diet, and blood sugar even when the exact word is absent.")
        st.markdown("> **Best for:** Broad research and discovering related principles.")

        st.markdown("---")

        st.markdown("##### LEXICAL SEARCH // Exact Match ON ")
        st.markdown("Searches for direct text matches in Thread Title and Post Content.")
        st.markdown("- **Multiple Words (AND):** `TEST MAST` returns posts containing both terms.\n- **Exact Phrase:** Use quotes, eg., `STIFF LEG`, to match the phrase exactly.")
        st.markdown("> **Best for:** Pinpointing specific dosages, protocols, or literal forum queries.")

        st.markdown("---")

        st.markdown("##### INTERFACE CONTROLS")
        st.markdown("- **Sort:** Relevance (vector distance), Newest, or Oldest.\n- **Result Scope:** Controls retrieval depth (Top 100 vs All Matches).\n- **Page Size:** Controls how many entries render per page.")

    col_scope, col_sort, col_page, col_exact = st.columns([1, 1, 1, 1])
    with col_scope:
        result_scope = st.selectbox(
            "Result Scope",
            options=["All matches", "Top 50", "Top 100", "Top 200", "Top 500"],
            index=0,
        )
    with col_sort:
        sort_by = st.selectbox("Sort", options=["Relevance", "Newest", "Oldest"], index=0)
    with col_page:
        page_size = st.selectbox("Page Size", options=[10, 20, 50, 100], index=1)
    with col_exact:
        exact_match_val = st.selectbox("Exact Match", options=["OFF", "ON"], index=0)
        exact_match = exact_match_val == "ON"

    search_query = st.text_input(
        "Search the Logbook",
        placeholder="Use quotes for exact phrase: \"trap bar\" rdl",
    )

    search_clicked = st.button("Search", type="primary")

    scope_to_k = {
        "All matches": 0,
        "Top 50": 50,
        "Top 100": 100,
        "Top 200": 200,
        "Top 500": 500,
    }
    search_signature = (search_query.strip(), result_scope, sort_by, exact_match)

    if search_clicked:
        if not search_query.strip():
            st.warning("Enter a search query first.")
        else:
            with st.spinner(f"Searching for '{search_query}'..."):
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
                    st.error(f"Search failed: {e}")

    if st.session_state.consultant_signature and search_signature != st.session_state.consultant_signature:
        st.caption("Filters changed. Click Search to refresh results.")

    total_results = len(st.session_state.consultant_results)
    if total_results > 0:
        total_pages = max(1, math.ceil(total_results / page_size))
        st.session_state.page_index = min(st.session_state.page_index, total_pages - 1)

        start_index = st.session_state.page_index * page_size
        end_index = min(start_index + page_size, total_results)

        nav_prev, nav_meta, nav_next = st.columns([1, 2, 1])
        with nav_prev:
            if st.button("Previous", disabled=st.session_state.page_index <= 0):
                st.session_state.page_index -= 1
                st.rerun()
        with nav_meta:
            st.markdown(
                f"**Showing {start_index + 1}-{end_index} of {total_results}**  "
                + f"(Page {st.session_state.page_index + 1}/{total_pages})"
            )
        with nav_next:
            if st.button("Next", disabled=st.session_state.page_index >= total_pages - 1):
                st.session_state.page_index += 1
                st.rerun()

        for doc in st.session_state.consultant_results[start_index:end_index]:
            render_source_document(doc, highlight_query=st.session_state.consultant_active_query)
    elif st.session_state.consultant_signature:
        st.warning("No matching entries found for this query.")

elif mode == "Oracle //Chat":
    for message in st.session_state.messages:
        with st.chat_message(message["role"]):
            st.markdown(message["content"])

    if st.session_state.focus_doc:
        st.success(
            f"🔍 **Focus Mode Active:** Analyzing Post ID {st.session_state.focus_doc.metadata.get('post_id')}"
        )
        if st.button("Clear Focus"):
            st.session_state.focus_doc = None
            st.rerun()

    if prompt := st.chat_input("Ask Jordan..."):
        st.session_state.messages.append({"role": "user", "content": prompt})
        with st.chat_message("user"):
            st.markdown(prompt)

        chat_history = []
        user_msg = None
        for msg in st.session_state.messages[:-1]:
            if msg["role"] == "user":
                user_msg = msg["content"]
            elif msg["role"] == "assistant" and user_msg:
                chat_history.append((user_msg, msg["content"]))
                user_msg = None

        with st.chat_message("assistant"):
            with st.spinner("Synthesising..."):
                try:
                    if st.session_state.focus_doc:
                        custom_prompt = qa_chain.combine_docs_chain.llm_chain.prompt.format(
                            context=st.session_state.focus_doc.page_content,
                            question=prompt,
                        )
                        raw_response = qa_chain.combine_docs_chain.llm_chain.llm.invoke(custom_prompt)
                        answer = getattr(raw_response, "content", str(raw_response))
                        st.markdown(answer)
                        st.session_state.messages.append({"role": "assistant", "content": answer})
                        render_source_document(st.session_state.focus_doc, show_send_button=False)
                    else:
                        response = qa_chain.invoke({"question": prompt, "chat_history": chat_history})
                        answer = response.get("answer", "No answer generated.")
                        st.markdown(answer)
                        st.session_state.messages.append({"role": "assistant", "content": answer})

                        source_docs = response.get("source_documents", [])
                        if source_docs:
                            st.caption("Source Logs:")
                            for doc in source_docs:
                                render_source_document(doc, show_send_button=False)

                except Exception as e:
                    st.error(f"Query failed: {e}")
