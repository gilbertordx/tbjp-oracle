//! TBJP Oracle (ARCHIVE): search Jordan Peters' forum archive.
//!
//! Rust remaster of the Python app. Module map (Python file -> Rust file):
//!
//! | Python                       | Rust                |
//! |------------------------------|---------------------|
//! | `src/schema.py`              | `models.rs`         |
//! | `src/pipeline.py`            | `chunker.rs`        |
//! | `src/vector_store.py`        | `store.rs`          |
//! | `ingest.py`, `mock_data_gen.py` | `ingest.rs`      |
//! | `src/scraper.py`             | `scraper.rs`        |
//! | `src/llm_engine.py`          | `llm.rs`            |
//! | `app.py` (Streamlit)         | `web.rs`            |
//! | date parsing (3 copies)      | `dates.rs`          |
//! | `main.py` (empty)            | `main.rs` (the CLI) |

pub mod chunker;
pub mod dates;
pub mod ingest;
pub mod llm;
pub mod models;
pub mod scraper;
pub mod store;
pub mod web;
