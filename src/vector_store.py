from __future__ import annotations

import json
import re
from datetime import datetime
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
                "`langchain-huggingface` and/or `chromadb` are unavailable. "
                "Search uses keyword matching; semantic search is unavailable."
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

    def raw_search(self, query: str, k: int = 20):
        """Returns raw documents for the Consultant/Wiki mode."""
        if self.backend == "chroma":
            return self.db.similarity_search(query, k=k)
        return _KeywordRetriever(self._fallback_documents, top_k=k).invoke(query)

    def hybrid_search(
        self,
        query: str,
        k: int = 20,
        exact_match: bool = False,
        sort_by: str = "relevance",
    ) -> list[Document]:
        """
        Hybrid retrieval for Consultant mode.
        - Over-fetches semantic candidates (5x k)
        - Re-hydrates full post text by grouping chunks per post_id
        - Applies boolean lexical filtering when exact_match is enabled
        - Supports relevance/newest/oldest sorting
        """
        normalized_query = query.strip()
        if not normalized_query:
            return []

        if k <= 0:
            candidate_k = self._count_documents()
        else:
            candidate_k = max(k * 5, k)

        scored_candidates = self._get_scored_candidates(normalized_query, candidate_k)
        if not scored_candidates:
            return []

        # Chroma returns lower score = better match. Keep deterministic order.
        scored_candidates.sort(key=lambda item: item[1])

        post_groups = self._rehydrate_posts(scored_candidates)
        if exact_match:
            post_groups = [
                group
                for group in post_groups
                if self._matches_query_terms(
                    " ".join(
                        [
                            str(group["document"].metadata.get("thread_title", "")),
                            group["document"].page_content,
                        ]
                    ),
                    normalized_query,
                )
            ]

        if sort_by == "newest":
            post_groups.sort(
                key=lambda group: self._extract_document_datetime(group["document"]),
                reverse=True,
            )
        elif sort_by == "oldest":
            post_groups.sort(
                key=lambda group: self._extract_document_datetime(group["document"]),
            )
        else:
            post_groups.sort(key=lambda group: (group["best_score"], group["first_rank"]))

        documents = [group["document"] for group in post_groups]
        if k <= 0:
            return documents
        return documents[:k]

    def _get_scored_candidates(self, query: str, candidate_k: int) -> list[tuple[Document, float]]:
        if self.backend == "chroma":
            return self.db.similarity_search_with_score(query, k=candidate_k)

        docs = _KeywordRetriever(self._fallback_documents, top_k=candidate_k).invoke(query)
        return [(doc, float(index)) for index, doc in enumerate(docs)]

    def _count_documents(self) -> int:
        if self.backend == "chroma":
            try:
                return int(self.db._collection.count())  # noqa: SLF001
            except Exception:
                try:
                    payload = self.db.get(include=[])
                    return len(payload.get("ids", []))
                except Exception:
                    return 20000
        return len(self._fallback_documents)

    @staticmethod
    def _rehydrate_posts(scored_candidates: list[tuple[Document, float]]) -> list[dict]:
        grouped: dict[str, dict] = {}
        fallback_index = 0

        for rank, (doc, score) in enumerate(scored_candidates):
            post_id = str(doc.metadata.get("post_id", "")).strip()
            if not post_id:
                post_id = f"_missing_post_{fallback_index}"
                fallback_index += 1

            if post_id not in grouped:
                grouped[post_id] = {
                    "metadata": dict(doc.metadata),
                    "chunks": [],
                    "chunk_seen": set(),
                    "best_score": score,
                    "first_rank": rank,
                }
            else:
                grouped[post_id]["best_score"] = min(grouped[post_id]["best_score"], score)
                grouped[post_id]["first_rank"] = min(grouped[post_id]["first_rank"], rank)

            chunk_text = doc.page_content.strip()
            if chunk_text and chunk_text not in grouped[post_id]["chunk_seen"]:
                grouped[post_id]["chunks"].append(chunk_text)
                grouped[post_id]["chunk_seen"].add(chunk_text)

        post_groups: list[dict] = []
        for payload in grouped.values():
            combined_content = "\n\n".join(payload["chunks"]).strip()
            post_doc = Document(
                page_content=combined_content,
                metadata=payload["metadata"],
            )
            post_groups.append(
                {
                    "document": post_doc,
                    "best_score": payload["best_score"],
                    "first_rank": payload["first_rank"],
                }
            )

        return post_groups

    @staticmethod
    def _parse_query_requirements(query: str) -> tuple[list[str], list[str]]:
        phrases: list[str] = []
        terms: list[str] = []

        for phrase in re.findall(r'"([^"]+)"', query):
            cleaned = phrase.strip().lower()
            if cleaned:
                phrases.append(cleaned)

        remainder = re.sub(r'"[^"]+"', " ", query)
        for token in re.findall(r"[a-z0-9]+", remainder.lower()):
            if token == "and":
                continue
            terms.append(token)

        return phrases, terms

    def _matches_query_terms(self, text: str, query: str) -> bool:
        searchable = text.lower()
        phrases, terms = self._parse_query_requirements(query)

        if not phrases and not terms:
            return True

        for phrase in phrases:
            if phrase not in searchable:
                return False

        for term in terms:
            if not re.search(rf"\b{re.escape(term)}\b", searchable):
                return False

        return True

    @staticmethod
    def _extract_document_datetime(doc: Document) -> datetime:
        raw_timestamp = doc.metadata.get("timestamp")
        if isinstance(raw_timestamp, str):
            try:
                return datetime.fromisoformat(raw_timestamp)
            except ValueError:
                pass

        raw_date = doc.metadata.get("date")
        if isinstance(raw_date, str):
            normalized = " ".join(raw_date.split())
            try:
                return datetime.strptime(normalized, "%B %d, %Y at %I:%M %p")
            except ValueError:
                pass

        return datetime.min

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
