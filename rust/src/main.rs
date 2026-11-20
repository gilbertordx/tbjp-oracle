//! Command-line entry point. Run `tbjp-oracle --help` for the commands.
//!
//! The Python project had no single entry point (`main.py` was empty): you ran
//! `python src/scraper.py`, `python ingest.py`, `streamlit run app.py`, etc.
//! Here they are subcommands of one program.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use tbjp_oracle::ingest::{build_chunks, load_raw_posts, mock_posts};
use tbjp_oracle::llm::ArchiveLlm;
use tbjp_oracle::store::{SortBy, Store};
use tbjp_oracle::web::AppState;
use tbjp_oracle::{chunker, scraper};

#[derive(Parser)]
#[command(name = "tbjp-oracle", version, about = "Search Jordan Peters' forum archive")]
struct Cli {
    /// Folder holding raw_forum_data.json and the index/ folder.
    #[arg(long, global = true, default_value = "data")]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, ValueEnum)]
enum SortArg {
    Relevance,
    Newest,
    Oldest,
}

#[derive(Subcommand)]
enum Command {
    /// Download forum replies into data/raw_forum_data.json (hits the live site).
    Scrape {
        /// How many listing pages to fetch (the original script used 1035).
        #[arg(long, default_value_t = 1)]
        pages: u32,
        /// Seconds to wait between pages.
        #[arg(long, default_value_t = 2)]
        delay: u64,
    },
    /// Chunk the raw JSON and (re)build the search index.
    Ingest,
    /// Add three sample posts to the index (no scraping needed).
    Seed,
    /// Search from the terminal.
    Search {
        query: String,
        /// Require every word / quoted phrase.
        #[arg(long)]
        exact: bool,
        #[arg(long, value_enum, default_value = "relevance")]
        sort: SortArg,
        /// Max posts to show (0 = all).
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Start the web UI.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8501")]
        addr: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let raw_path = cli.data_dir.join("raw_forum_data.json");
    let index_path = cli.data_dir.join("index").join("docs.jsonl");

    match cli.command {
        Command::Scrape { pages, delay } => scraper::run(pages, delay, &raw_path)?,

        Command::Ingest => {
            println!("Loading JSON data...");
            let raw = load_raw_posts(&raw_path)?;
            println!("Parsing {} posts...", raw.len());
            let (chunks, skipped) = build_chunks(&raw);
            println!("Prepared {} chunks. Skipped posts: {skipped}", chunks.len());
            let mut store = Store::open(&index_path)?;
            store.upsert(chunks)?;
            println!("Ingestion complete. Index now holds {} chunks.", store.len());
        }

        Command::Seed => {
            let chunks: Vec<_> = mock_posts().iter().flat_map(chunker::process_forum_post).collect();
            let count = chunks.len();
            let mut store = Store::open(&index_path)?;
            store.upsert(chunks)?;
            println!("Seed complete: {count} chunks indexed.");
        }

        Command::Search { query, exact, sort, limit } => {
            let store = Store::open(&index_path)?;
            let sort_by = match sort {
                SortArg::Relevance => SortBy::Relevance,
                SortArg::Newest => SortBy::Newest,
                SortArg::Oldest => SortBy::Oldest,
            };
            let results = store.hybrid_search(&query, limit, exact, sort_by);
            if results.is_empty() {
                println!("no matching entries found for this query.");
            }
            for doc in &results {
                let m = &doc.metadata;
                println!(
                    "--- {} | {} (id: {})",
                    m.date.as_deref().unwrap_or("unknown date"),
                    m.thread_title.as_deref().unwrap_or("unknown thread"),
                    m.post_id.as_deref().unwrap_or("unknown id"),
                );
                println!("{}\n", doc.page_content);
            }
        }

        Command::Serve { addr } => {
            let state = AppState { store: Store::open(&index_path)?, llm: ArchiveLlm::from_env() };
            // The web server is `async`, so it needs a runtime to drive it.
            // Everything else in this program is plain synchronous code.
            tokio::runtime::Runtime::new()?.block_on(tbjp_oracle::web::serve(state, &addr))?;
        }
    }
    Ok(())
}
