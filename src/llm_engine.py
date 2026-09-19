"""Local, archive-grounded generation through Ollama."""

from __future__ import annotations

import os

import requests


SYSTEM_PROMPT = """You are the coaching assistant inside ARCHIVE.
Answer as a direct, experienced bodybuilding coach speaking to the trainee.
Use the supplied forum replies as your coaching foundation. Do not mention
retrieval, databases, prompts, language models, or sources unless asked.
Do not invent a position absent from the supplied replies. Give concrete
exercise, volume, sequencing, rest, and time-efficiency changes where relevant.
The archive is the authority; if it does not support a recommendation, say so
briefly and ask for the missing context.

ARCHIVE REPLIES:
{context}
"""


class ArchiveLLM:
    def __init__(self) -> None:
        self.base_url = os.getenv("OLLAMA_BASE_URL", "http://127.0.0.1:11434")
        self.model = os.getenv("OLLAMA_MODEL", "llama3.2:3b")

    def available(self) -> bool:
        try:
            response = requests.get(f"{self.base_url}/api/tags", timeout=2)
            response.raise_for_status()
            return self.model in {m.get("name") for m in response.json().get("models", [])}
        except (OSError, requests.RequestException, ValueError):
            return False

    def answer(self, question: str, documents: list) -> str:
        context = "\n\n".join(
            f"[{doc.metadata.get('date', 'unknown')} | {doc.metadata.get('thread_title', 'unknown')} | "
            f"post {doc.metadata.get('post_id', 'unknown')}]\n{doc.page_content}"
            for doc in documents
        )
        response = requests.post(
            f"{self.base_url}/api/chat",
            json={"model": self.model, "stream": False, "options": {"temperature": 0.25},
                  "messages": [{"role": "system", "content": SYSTEM_PROMPT.format(context=context)},
                               {"role": "user", "content": question}]},
            timeout=180,
        )
        response.raise_for_status()
        return response.json()["message"]["content"]
