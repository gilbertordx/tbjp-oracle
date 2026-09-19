# ARCHIVE

Search the local forum archive without an LLM API key. Gemini has been removed;
rag mode *experimental is paused pending selection of a free replacement provider.

## Run locally

```sh
python3 -m venv venv
venv/bin/pip install streamlit python-dotenv langchain-core langchain-text-splitters
venv/bin/python ingest.py
venv/bin/streamlit run app.py --server.address 127.0.0.1
```

The minimal installation uses the existing keyword retrieval backend. The UI
shows a warning that semantic search is unavailable. Ingestion preserves source
metadata and writes the local ignored `data/chroma_db/fallback_docs.jsonl` index.
Stop Streamlit before running ingestion.

For semantic retrieval, install `requirements.txt` and ingest again with that
backend before launching the app. This downloads the embedding model locally.

## Data pipeline

This project evolved from [tbjp-scraper](https://github.com/gilbertordx/tbjp-scraper).

`src/scraper.py` → `data/raw_forum_data.json` → `ingest.py` → vector store → `app.py`

- `src/scraper.py` downloads forum replies and backs up existing raw data before overwriting it.
- `src/schema.py` defines forum posts; `src/pipeline.py` chunks long posts.
- `ingest.py` preserves source metadata and ingests in batches of 5,000.
- `src/vector_store.py` provides Chroma semantic retrieval or keyword retrieval.
- `mock_data_gen.py` seeds sample posts for local testing.
- `src/llm_engine.py` records the paused experimental feature; no model is currently configured.

Run `venv/bin/python src/scraper.py` only when you intend to refresh the forum data.
rag mode *experimental is unavailable pending selection of a replacement
model.
