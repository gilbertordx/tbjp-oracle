# TBJP Oracle

A retrieval-augmented search and chat tool over the forum archive of bodybuilding coach **Jordan Peters** (Trained by JP). Forum replies are scraped, chunked, embedded into a vector store, and served through a two-mode Streamlit app: a raw hybrid-search "Consultant" view for browsing the logbook directly, and an "Oracle" chat view that answers questions grounded strictly in retrieved posts, written in Jordan's own voice.

This project evolved from an earlier standalone scraper, [`tbjp-scraper`](https://github.com/gilbertordx/tbjp-scraper), which now lives on here as `src/scraper.py`.

## How it works

```
src/scraper.py  →  data/raw_forum_data.json  →  ingest.py  →  vector store (Chroma)  →  app.py (Streamlit)
```

1. **Scrape** — `src/scraper.py` pulls reply posts from trainedbyjp.com, extracting post/thread IDs, titles, dates, and cleaned content.
2. **Ingest** — `ingest.py` parses each post, chunks long ones (`src/pipeline.py`), and writes them into a Chroma vector store (`src/vector_store.py`), embedded with `BAAI/bge-large-en-v1.5`.
3. **Serve** — `app.py` exposes two modes:
   - **Consultant //Wiki** — hybrid semantic + lexical search over the raw archive, no LLM involved.
   - **Oracle //Chat** — conversational RAG via Gemini (`gemini-2.5-flash`), answering only from retrieved context and stating plainly when something isn't in the logbook.

If `chromadb` / `langchain-huggingface` aren't installed, the vector store transparently falls back to an in-memory keyword retriever, so the app still runs without the full embedding stack.

## Setup

```bash
pip install -r requirements.txt
```

`src/llm_engine.py` uses `langchain-google-genai`, which isn't currently listed in `requirements.txt` — install it separately if you want the Oracle chat mode:

```bash
pip install langchain-google-genai
```

Oracle chat requires a `.env` file with your Gemini credentials. Without it, the Consultant search mode still works:

```
GOOGLE_API_KEY=your-key-here
```

## Usage

**Seed the vector store with a handful of mock posts** (fastest way to try the app without scraping):

```bash
python mock_data_gen.py
```

**Scrape the live forum** (writes/overwrites `data/raw_forum_data.json`, backing up any existing file first):

```bash
python src/scraper.py
```

**Ingest scraped data into the vector store**:

```bash
python ingest.py
```

**Run the app**:

```bash
streamlit run app.py
```

If you have no Google API key, leave `.env` absent. The app will show only the Consultant search mode. To run against the bundled raw archive, run `python ingest.py` once before starting Streamlit. If Chroma's embedding packages are unavailable, ingestion uses the local keyword fallback instead of semantic search.

> Don't run `ingest.py` and `streamlit run app.py` at the same time — both can write to the Chroma store at `data/chroma_db`, and concurrent writes will corrupt it.

## Project layout

```
app.py                  Streamlit UI (Consultant + Oracle modes)
ingest.py                Loads data/raw_forum_data.json into the vector store
mock_data_gen.py         Seeds the vector store with sample posts for local testing
src/
  scraper.py              Forum scraper (trainedbyjp.com)
  schema.py                ForumPost data model
  pipeline.py               Chunking of long posts into Documents
  vector_store.py            Chroma-backed store with keyword fallback + hybrid search
  llm_engine.py               Gemini-backed conversational RAG chain
data/
  raw_forum_data.json      Scraped output (gitignored on refresh, backups kept alongside)
  chroma_db/                Persisted vector store (gitignored)
```

## Status

Proof of concept. No test suite yet.
