# ARCHIVE

Search the local forum archive without an LLM API key. Gemini has been removed;
Ask the Archive is paused pending selection of a free replacement provider.

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
