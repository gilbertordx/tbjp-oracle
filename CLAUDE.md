# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TBJP Oracle: a RAG proof-of-concept over the forum archive of bodybuilding coach Jordan Peters (Trained by JP). Forum replies are scraped, chunked, embedded into a vector store, and served through a two-mode Streamlit app: a raw hybrid-search "Consultant" view and an LLM chat "Oracle" view that answers strictly from retrieved context.

## Commands

```bash
pip install -r requirements.txt
```

- **Scrape the forum**: `python src/scraper.py` — hits `trainedbyjp.com`, defaults to `pages_to_scrape=1035` (edit the `__main__` block to scrape fewer pages). Overwrites `data/raw_forum_data.json`, timestamping a backup of any existing file first.
- **Seed the vector store with mock data** (no scrape/network needed): `python mock_data_gen.py`
- **Ingest scraped data into the vector store**: `python ingest.py` — reads `data/raw_forum_data.json`, batches writes in groups of 5000 into Chroma at `data/chroma_db`.
- **Run the UI**: `streamlit run app.py`
- `main.py` is currently empty (not an entry point).
- There is no test suite and no linter config in this repo.

### Operational constraints
- **Single-writer discipline**: never run `ingest.py` and `streamlit run app.py` at the same time — both can touch the Chroma/SQLite store at `data/chroma_db` and concurrent writes corrupt it.
- If ingestion fails mid-write, delete `data/chroma_db` and rebuild from scratch rather than debugging a partial store, unless you're specifically diagnosing corruption.
- `.env` holds secrets (`GOOGLE_API_KEY` for Gemini, loaded via `load_dotenv()` in `app.py`/`ingest.py` paths). Never hardcode credentials in source.

## Architecture

**Pipeline**: `src/scraper.py` → `data/raw_forum_data.json` → `ingest.py` → vector store.

- `src/scraper.py` scrapes `div.bs-reply-list-item` reply blocks, pulling `post_id`/`thread_id` out of DOM class names (`post-<id>`, `bbp-parent-topic-<id>`), thread title from `a.bbp-topic-permalink`, and a historical date string from `span.bs-timestamp` (format `%B %d, %Y at %I:%M %p`). Records missing either ID are skipped and logged; unparseable dates keep the raw string with `timestamp=null`.
- `ingest.py` re-parses each raw record into a `ForumPost` (`src/schema.py`), re-attempting timestamp parsing if needed and skipping posts that still have no valid timestamp. Valid posts go through `process_forum_post` (`src/pipeline.py`), which only chunks content that exceeds 2000 chars (`RecursiveCharacterTextSplitter`, 200 overlap) — most single posts pass through as one chunk. Source metadata (`post_id`, `thread_id`, `author`, `date`, `thread_title`) is re-attached to every chunk after splitting, since the splitter's own metadata is minimal.
- `src/vector_store.py` (`TBJPVectorStore`) wraps a Chroma collection (`BAAI/bge-large-en-v1.5` embeddings via `langchain-huggingface`) persisted at `data/chroma_db`. If `chromadb`/`langchain-huggingface` aren't importable, it transparently drops to a `backend="fallback"` mode: an in-memory keyword-overlap retriever (`_KeywordRetriever`) backed by a JSONL file (`data/chroma_db/fallback_docs.jsonl`). Code calling into the store (UI, ingest) doesn't need to know which backend is active — same `ingest_documents`/`get_retriever`/`hybrid_search` interface either way.
- `hybrid_search` is the retrieval path behind the Consultant UI: it over-fetches semantic candidates (5x the requested `k`), **re-hydrates full posts** by grouping/concatenating chunks back together per `post_id` (so users see whole posts, not fragments), optionally applies boolean lexical filtering (quoted phrases + AND'd terms) when exact-match is on, then sorts by relevance/newest/oldest.

**LLM layer**: `src/llm_engine.py` (`TBJPOracleLLM`) wraps Gemini (`gemini-2.5-flash` via `ChatGoogleGenerativeAI`) in a `ConversationalRetrievalChain`, with a system prompt that locks the model into Jordan Peters' persona/voice and forbids answering outside retrieved context. Note: `langchain-google-genai` is imported here but is **not listed in `requirements.txt`** — install it separately to run Oracle chat mode.

**UI**: `app.py` is a single-file Streamlit app with two modes toggled via `st.sidebar.radio`, held in `st.session_state`:
- **Consultant //Wiki** — direct `hybrid_search` results, no LLM involved. Supports scope/sort/page-size/exact-match controls and highlights matched terms/phrases in results.
- **Oracle //Chat** — conversational RAG through `TBJPOracleLLM`'s chain, with turn history reconstructed from `st.session_state.messages`. A result card in Consultant mode can be sent to Oracle mode as a pinned "focus doc," which routes the next question through a direct prompt against just that document instead of the full retriever.

Both UI resources (`TBJPVectorStore`, the QA chain) are built once via `@st.cache_resource` in `init_system()`.
