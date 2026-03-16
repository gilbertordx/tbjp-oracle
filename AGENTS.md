# AGENTS.md - TBJP Oracle Operating Standard

This file defines the default engineering protocol for Codex working in this repository.
Scope: `d:\Gilberto\tbjp-oracle`

## 1. Core Engineering Rules
- Prioritise `SOLID`, `DRY`, and `KISS`.
- Prefer explicit, deterministic behavior over convenience shortcuts.
- Do not fabricate data when source fields are missing; log and skip where integrity matters.
- Use typed, clear Python code with meaningful names and small, testable units.
- Use f-strings with inline variables when formatting strings.
- Do not create one-off helper functions referenced only once unless they materially improve readability.

## 2. Stack and Entry Points
- Language/runtime: Python.
- UI: Streamlit (`app.py`).
- Scraper: `src/scraper.py`.
- Ingestion entrypoint: `ingest.py`.
- Schema/chunking: `src/schema.py`, `src/pipeline.py`.
- Retrieval store: `src/vector_store.py` (Chroma primary, fallback retriever secondary).
- LLM orchestration: `src/llm_engine.py`.

## 3. Dependency and Environment Policy
- Install missing project dependencies before running workflow steps.
- Keep secrets only in `.env`; never hardcode API keys in source.
- Never commit real credentials or paste production keys into tracked files.
- For this repo's constrained network environment, honour proxy bypass patterns already established in project commands.

## 4. Scraper Standards (`src/scraper.py`)
- Target `div.bs-reply-list-item` for replies.
- Extract IDs from DOM classes:
  - `post-<id>` -> `post_id`
  - `bbp-parent-topic-<id>` -> `thread_id`
- Extract thread title from `a.bbp-topic-permalink`.
- Extract historical date text from `span.bs-timestamp`.
- Parse historical date to ISO timestamp using:
  - `%B %d, %Y at %I:%M %p`
- If parse fails:
  - keep raw `date`
  - set `timestamp` to `null`
- If `post_id` or `thread_id` is missing:
  - skip record
  - log page URL + item index
- Before overwriting `data/raw_forum_data.json`, create a timestamped backup file.
- Use throttle and retry logic to reduce bans/timeouts.

## 5. Ingestion Standards (`ingest.py`)
- Parse each post into `ForumPost` and chunk via `process_forum_post`.
- Preserve metadata from raw JSON onto every chunk:
  - `post_id`, `thread_id`, `author`, `date`, `thread_title`
- If `timestamp` is invalid/missing:
  - attempt parse from `date`
  - if still invalid, skip and log record
- Ingest in batches to avoid SQLite/Chroma transaction limits:
  - default batch size: `5000`
- Log totals:
  - raw posts
  - skipped posts
  - prepared chunks
  - batch completion progress

## 6. Chroma/SQLite Operational Safety
- Single-writer discipline: never run ingestion and Streamlit concurrently.
- If ingestion fails mid-write, purge `data/chroma_db` before rebuild unless explicitly debugging corruption.
- Avoid detached background Streamlit launches when diagnosing DB issues.
- Run long operations in foreground when logs are required.

## 7. Streamlit UI Standards (`app.py`)
- Load environment variables at startup (`load_dotenv()`).
- Cache expensive resources with `@st.cache_resource`:
  - vector store init
  - retriever
  - QA chain
- Preserve conversational memory via session history for conversational retrieval.
- Show source evidence in expanders with metadata label:
  - `<date> | <thread_title> (ID: <post_id>)`
- Display raw `doc.page_content` for source transparency.

## 8. LLM Safety and Behavior
- Responses must be grounded in retrieved context only.
- If context is insufficient, explicitly say answer is not in the logbook.
- No invented citations, protocols, or fabricated confidence.
- Keep generation settings stable and deterministic enough for production validation.

## 9. Validation Checklist Before Sign-Off
- Scraper smoke test (2 pages): verify `post_id`, `thread_id`, `thread_title`, `date`.
- Full scrape: validate high post count and backup creation.
- Ingestion: confirm `data/chroma_db/chroma.sqlite3` exists and batch run completes.
- Retrieval probe: confirm returned `doc.metadata` includes `date` and `thread_title`.
- UI probe: source expanders should not default to `Unknown Date` / `Unknown Thread` except genuine source gaps.
- Conversational follow-up test: verify chain uses prior turn context.

## 10. Documentation and Change Hygiene
- When data contract changes (scraper output, chunk metadata, retrieval payload), update docs/readme notes in same change.
- Keep commits scoped and traceable (scraper, ingest, UI, infra).
- Do not silently alter behavior with hidden fallbacks; log major decision points.
