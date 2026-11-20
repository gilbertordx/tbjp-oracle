//! Optional local-LLM answers through Ollama (the "rag mode *experimental").
//!
//! Python equivalent: `src/llm_engine.py`. Same environment variables
//! (`OLLAMA_BASE_URL`, `OLLAMA_MODEL`), same prompt, same 24,000-character
//! context cap. This is `async` because it is called from the web server.

use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};

use crate::models::Chunk;

const SYSTEM_PROMPT: &str = "You are the coaching assistant inside ARCHIVE.
Answer as a direct, experienced bodybuilding coach speaking to the trainee.
Use the supplied forum replies as your coaching foundation. Do not mention
retrieval, databases, prompts, language models, or sources unless asked.
Do not invent a position absent from the supplied replies. Give concrete
exercise, volume, sequencing, rest, and time-efficiency changes where relevant.
The archive is the authority; if it does not support a recommendation, say so
briefly and ask for the missing context.

ARCHIVE REPLIES:
{context}
";

pub struct ArchiveLlm {
    pub base_url: String,
    pub model: String,
    client: reqwest::Client,
}

impl ArchiveLlm {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
            model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:3b".into()),
            client: reqwest::Client::new(),
        }
    }

    /// Is Ollama running with our model installed?
    pub async fn available(&self) -> bool {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await;
        let Ok(response) = response else { return false };
        let Ok(body) = response.json::<Value>().await else { return false };
        body["models"]
            .as_array()
            .map(|models| models.iter().any(|m| m["name"] == self.model.as_str()))
            .unwrap_or(false)
    }

    /// Build the prompt context from retrieved posts (capped, by characters).
    pub fn build_context(documents: &[Chunk]) -> String {
        let joined = documents
            .iter()
            .map(|doc| {
                let m = &doc.metadata;
                format!(
                    "[{} | {} | post {}]\n{}",
                    m.date.as_deref().unwrap_or("unknown"),
                    m.thread_title.as_deref().unwrap_or("unknown"),
                    m.post_id.as_deref().unwrap_or("unknown"),
                    doc.page_content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        joined.chars().take(24_000).collect()
    }

    pub async fn answer(&self, question: &str, documents: &[Chunk]) -> Result<String> {
        let system = SYSTEM_PROMPT.replace("{context}", &Self::build_context(documents));
        let body = json!({
            "model": self.model,
            "stream": false,
            "options": { "temperature": 0.25 },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": question },
            ],
        });
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .timeout(Duration::from_secs(600))
            .send()
            .await?
            .error_for_status()?;
        let value: Value = response.json().await?;
        Ok(value["message"]["content"].as_str().unwrap_or_default().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Metadata;

    #[test]
    fn context_lists_source_and_text_and_is_capped() {
        let doc = Chunk {
            page_content: "x".repeat(30_000),
            metadata: Metadata {
                post_id: Some("7".into()),
                date: Some("March 2, 2026 at 3:12 pm".into()),
                thread_title: Some("Legs".into()),
                ..Default::default()
            },
        };
        let ctx = ArchiveLlm::build_context(&[doc]);
        assert!(ctx.starts_with("[March 2, 2026 at 3:12 pm | Legs | post 7]\n"));
        assert_eq!(ctx.chars().count(), 24_000);
    }
}
