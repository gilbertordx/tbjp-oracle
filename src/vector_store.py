from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Iterable

from langchain_core.documents import Document

try:
    from langchain_huggingface import HuggingFaceEmbeddings
    from langchain_community.vectorstores import Chroma
except ImportError:
    HuggingFaceEmbeddings = None
    Chroma = None


class _KeywordRetriever:
    def __init__(self, documents: Iterable[Document], top_k: int):
        self._documents = list(documents)
        self._top_k = top_k

    @staticmethod
    def _tokenize(value: str) -> set[str]:
        return set(re.findall(r"[a-z0-9]+", value.lower()))

    def invoke(self, query: str) -> list[Document]:
        query_terms = self._tokenize(query)
        if not query_terms:
            return []

        scored: list[tuple[int, Document]] = []
        for doc in self._documents:
            content_terms = self._tokenize(doc.page_content)
            score = len(query_terms.intersection(content_terms))
            if score > 0:
                scored.append((score, doc))

        scored.sort(key=lambda item: item[0], reverse=True)
        return [doc for _, doc in scored[: self._top_k]]


class TBJPVectorStore:
    def __init__(self, persist_directory: str = "./data/chroma_db"):
        self.persist_directory = Path(persist_directory)
        self.persist_directory.mkdir(parents=True, exist_ok=True)
        self._fallback_store_path = self.persist_directory / "fallback_docs.jsonl"
        self._fallback_documents: list[Document] = []

        if HuggingFaceEmbeddings and Chroma:
            # Set 'device' to 'cuda' (Nvidia) or 'mps' (Mac) if hardware allows.
            self.embeddings = HuggingFaceEmbeddings(
                model_name="BAAI/bge-large-en-v1.5",
                model_kwargs={"device": "cpu"},
            )
            self.db = Chroma(
                collection_name="tbjp_posts",
                embedding_function=self.embeddings,
                persist_directory=str(self.persist_directory),
            )
            self.backend = "chroma"
            self.status_message = ""
        else:
            self.embeddings = None
            self.db = None
            self.backend = "fallback"
            self.status_message = (
                "Running in fallback mode because optional packages "
                "`langchain-huggingface` and/or `chromadb` are unavailable."
            )
            self._load_fallback_documents()

    def ingest_documents(self, documents: list[Document]):
        if not documents:
            print("No documents provided for ingestion.")
            return

        if self.backend == "chroma":
            self.db.add_documents(documents)
        else:
            self._fallback_documents.extend(documents)
            self._persist_fallback_documents()

        print(f"Ingested {len(documents)} chunks into the vector store ({self.backend}).")

    def get_retriever(self, target_results: int = 5):
        if self.backend == "chroma":
            return self.db.as_retriever(search_kwargs={"k": target_results})
        return _KeywordRetriever(self._fallback_documents, top_k=target_results)

    def _load_fallback_documents(self):
        if not self._fallback_store_path.exists():
            return

        with self._fallback_store_path.open("r", encoding="utf-8") as file_obj:
            for raw_line in file_obj:
                line = raw_line.strip()
                if not line:
                    continue
                payload = json.loads(line)
                self._fallback_documents.append(
                    Document(
                        page_content=payload["page_content"],
                        metadata=payload.get("metadata", {}),
                    )
                )

    def _persist_fallback_documents(self):
        with self._fallback_store_path.open("w", encoding="utf-8") as file_obj:
            for doc in self._fallback_documents:
                file_obj.write(
                    json.dumps(
                        {
                            "page_content": doc.page_content,
                            "metadata": doc.metadata,
                        },
                        ensure_ascii=False,
                    )
                    + "\n"
                )
