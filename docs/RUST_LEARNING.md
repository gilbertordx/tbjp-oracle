# Learning Rust through TBJP Oracle

This branch (`rust-remaster`) contains a Rust rewrite of the Python ARCHIVE app in
[`rust/`](../rust). The Python code is untouched next to it so you can read both
versions side by side. This document is the guide for doing that: what maps to what,
which Rust ideas appear where, and what changed on purpose.

You do not need to write the code to learn from it. The goal is that you can
**read** any file in `rust/src/` and explain what it does and why Rust makes you
write it that way.

---

## 1. Running it

Run everything from the repository root, because the data lives in `data/`.

```bash
# one-time: Rust toolchain from https://rustup.rs  (cargo 1.97 was used here)
cargo build --release --manifest-path rust/Cargo.toml

# Build the index from data/raw_forum_data.json (about 6 s even in a debug build)
cargo run --release --manifest-path rust/Cargo.toml -- ingest

# Search from the terminal
cargo run --release --manifest-path rust/Cargo.toml -- search '"stiff leg" rdl' --exact --sort newest

# Web UI at http://127.0.0.1:8501 (replaces `streamlit run app.py`)
cargo run --release --manifest-path rust/Cargo.toml -- serve

# Tests (29 of them)
cargo test --manifest-path rust/Cargo.toml
```

Other commands: `seed` (three sample posts, no scraping) and `scrape` (hits the live
site; defaults to 1 page, not the old 1035 -- pass `--pages 1035` if you mean it).

---

## 2. Python file → Rust file

| Python | Rust | What it does |
|---|---|---|
| `src/schema.py` | `rust/src/models.rs` | The data shapes: a post, a chunk, its metadata |
| `src/pipeline.py` | `rust/src/chunker.rs` | Cuts long posts into overlapping chunks |
| `src/vector_store.py` | `rust/src/store.rs` | The index: save, load, keyword search, ranking |
| `ingest.py`, `mock_data_gen.py` | `rust/src/ingest.rs` | Raw JSON → chunks; the sample posts |
| `src/scraper.py` | `rust/src/scraper.rs` | Downloads and parses forum pages |
| `src/llm_engine.py` | `rust/src/llm.rs` | Optional local Ollama answers |
| `app.py` (Streamlit) | `rust/src/web.rs` | The browser UI |
| three copies of date parsing | `rust/src/dates.rs` | One shared date parser |
| `main.py` (empty) | `rust/src/main.rs` | One command-line program with subcommands |
| `requirements.txt` | `rust/Cargo.toml` | The dependency list |

Read them in this order: `models.rs` → `dates.rs` → `chunker.rs` → `store.rs` →
`ingest.rs` → `scraper.rs` → `web.rs` → `main.rs`. Each builds on the one before.

---

## 3. The Rust ideas, in the order you will meet them

Each section names the file where you can see the idea in real code.

### 3.1 Cargo and crates (`Cargo.toml`)
A **crate** is a package; **Cargo** is `pip` + `venv` + the test runner + the build tool
in one. `cargo add serde` is `pip install`, `cargo test` is `pytest`. Dependencies are
pinned in `Cargo.lock` automatically. Compare to `requirements.txt`: the Python app
needed LangChain, Chroma, sentence-transformers and Streamlit; this one needs
`serde`, `regex`, `chrono`, `clap`, `axum`, `reqwest` and `scraper`.

### 3.2 Structs instead of dicts and pydantic (`models.rs`)
Python passed `dict`s and a pydantic model around. Rust describes each shape once:

```rust
pub struct RawPost { pub post_id: String, pub thread_id: String, ... }
```

`#[derive(Deserialize, Serialize)]` above a struct asks the compiler to *generate* the
JSON reading/writing code. A misspelled field or a wrong type is a compile error, not a
`KeyError` at 2 a.m.

### 3.3 `Option` instead of `None` (`models.rs`, `ingest.rs`)
In Python any value can silently be `None`. In Rust a value that may be missing has the
type `Option<String>`, and you cannot use it without deciding what to do when it is
absent. `item.author.clone().unwrap_or_else(|| "Jordan Peters".to_string())` in
`ingest.rs` is the Rust form of Python's `item.get("author", "Jordan Peters")`.

### 3.4 `Result` and `?` instead of exceptions (`store.rs`, `ingest.rs`, `main.rs`)
Rust has no exceptions. A function that can fail returns `Result<T, Error>`. The `?`
operator means "if this failed, return the error to my caller now". Look at
`Store::open` in `store.rs`: opening a file, reading lines and parsing JSON can all
fail, and each failure is passed up with `?`. `.with_context(...)` (from the `anyhow`
crate) adds a human message such as "docs.jsonl: line 7 is not valid index JSON".
In `main.rs`, `fn main() -> Result<()>` prints any error and exits non-zero.

### 3.5 Ownership and borrowing: `String` vs `&str` (`chunker.rs`)
The idea Rust is famous for, and the thing that will feel strangest.
- `String` **owns** its text. `&str` is a **borrowed view** of text someone else owns.
- A function that only reads takes `&str`; one that must keep or return the text builds
  a new `String`.

In `chunker.rs`, `split_keep_separator(text: &str, ...) -> Vec<String>` borrows the
text, and returns brand-new owned pieces. The compiler proves nothing is used after it
is freed, so there is no garbage collector and no dangling reference.
`merge_splits` keeps a `Vec<&str>` (cheap views into pieces that already exist) so it
does not copy every piece again.

### 3.5b Characters are not bytes (`chunker.rs`)
Python's `len("’")` is 1. Rust's `"’".len()` is **3** (bytes of UTF-8). The forum text is
full of curly quotes and dashes, so `chunker.rs` defines `char_len` using
`s.chars().count()`. The test `counts_characters_not_bytes` guards this exact trap.
Getting it wrong would change where posts are split.

### 3.6 Iterators instead of list comprehensions (`store.rs`, `ingest.rs`)
Python: `[x for x in items if cond(x)]`. Rust chains iterator steps:

```rust
self.docs.iter().zip(&self.tokens)
    .filter_map(|(doc, words)| { let score = ...; (score > 0).then_some((score, doc)) })
    .collect()
```
`|a, b| ...` is a **closure** (Python `lambda`). `.collect()` is what finally builds the
`Vec`; nothing runs until then.

### 3.7 `match`, `if let`, `let ... else` (`ingest.rs`, `store.rs`)
`match` is Python's `match` but the compiler checks you covered every case. In
`ingest.rs`:

```rust
let Some(timestamp) = parse_timestamp(item) else { skipped += 1; continue; };
```
reads as: "get the timestamp, or skip this post". It is the Rust spelling of
`if not timestamp: skipped += 1; continue`.

### 3.8 Collections (`store.rs`)
`Vec<T>` = `list`, `HashMap<K, V>` = `dict`, `HashSet<T>` = `set`. Python dicts remember
insertion order; Rust's `HashMap` does **not**. `rehydrate_posts` therefore keeps a
`Vec<PostGroup>` for order and a `HashMap` only for lookup, to reproduce the Python
behaviour exactly.

### 3.9 Modules and visibility (`lib.rs`, every file)
Each file is a module. Everything is private unless marked `pub`. `lib.rs` lists the
modules; `main.rs` is just a thin command-line shell that calls into the library, so
the library can be tested without running the program.

### 3.10 Tests live next to the code (`#[cfg(test)] mod tests`)
At the bottom of each file. `cargo test` runs them all. No pytest, no fixtures folder.
`scraper.rs` tests the HTML parser on a small HTML string so **no network is needed**;
that is why parsing and downloading are separate functions.

### 3.11 `async` only where it is needed (`web.rs`, `llm.rs`, `main.rs`)
A web server handles many requests at once, which is what `async` is for. Everything
else here (ingesting, searching, scraping) is plain synchronous code. `main.rs` starts
the async runtime only inside the `serve` command. `Arc<AppState>` in `web.rs` is how
all the request handlers share one read-only copy of the loaded index.

### 3.12 Escaping HTML (`web.rs`)
Anything a user typed is passed through `escape` before it goes into HTML. The test
`highlight_escapes_the_page_text` shows `<script>` coming out harmless.

---

## 4. Differences from the Python version (deliberate)

Each of these changes behaviour, so they are listed instead of hidden.

1. **Streamlit → a small web server.** State (query, filters, page number) now lives
   in the URL instead of `st.session_state`. Results are bookmarkable and Back works.
   The look is simpler than Streamlit's.
2. **No Chroma / semantic search.** The Python app ran its keyword fallback on this
   installation anyway (the README says so), and that is what was ported. Semantic
   search would need an embedding model in Rust (the `fastembed` crate is the
   standard route). It is not done.
3. **`ingest` is repeatable.** Python's `ingest.py` appended, so running it twice
   stored every chunk twice. Here chunks are replaced by `post_id`, so you can run it
   as often as you like, and `seed` can add sample posts without wiping real data.
4. **Index location.** `data/index/docs.jsonl` instead of
   `data/chroma_db/fallback_docs.jsonl`. The line format is identical, so the old file
   can be copied over and it loads (that is how the verification below was done).
5. **Missing `thread_title` / `author`.** If a scraped record has `null` there, Python
   stored `None` (and the pydantic model rejected a null author). Rust uses the same
   defaults as for a *missing* key: `"Unknown Thread"` / `"Jordan Peters"`.
6. **Scraper default is 1 page**, not 1035, because it hits a live website.
   `--pages` and `--delay` are now command-line flags instead of editing the file.
7. **Dropped batching.** The 5000-per-batch writes existed for Chroma's limits.

---

## 5. How correctness was checked

Not just "it compiles":

- 29 unit tests (`cargo test`).
- **Chunker vs. the real LangChain splitter:** the original Python ingest logic
  (including `RecursiveCharacterTextSplitter`) and the Rust ingest were both run on all
  15,515 real posts. Result: **15,609 chunks each, 0 differences** in text or
  metadata, including the 91 posts longer than 2,000 characters that actually exercise
  the splitter.
- **Search vs. the original `hybrid_search`:** see section 6 for the result.

---

## 6. Search parity result

The original Python `TBJPVectorStore.hybrid_search` (keyword backend) and the Rust
`search` command were run against the same index and compared on the order of the
returned post ids: 12 queries (single words, quoted phrases, `and`, no-match, mixed
case) x exact match on/off x sort relevance/newest/oldest x limit 10/all =
**144 configurations, 0 mismatches.**

The web UI itself (layout, paging, highlighting, HTML escaping, the rag page when
Ollama is absent) was checked by hand against a running server, not against Streamlit.

---

## 7. Exercises (when you want to go deeper)

1. In `store.rs`, change the score so a *phrase* match counts double. Which test
   breaks? Which new test would you write?
2. Make `chunker.rs` take `chunk_size` as a function argument instead of a `const`.
   Notice how many call sites the compiler points you to.
3. Add a `--json` flag to `search` that prints results as JSON. (Hint: `Chunk` already
   derives `Serialize`.)
4. Add a `/healthz` route to `web.rs` that returns the number of chunks in the index.
5. Try deleting a `&` or a `.clone()` somewhere and read the compiler's message. Rust's
   error messages are a big part of how it teaches.

Good beginner references: *The Rust Programming Language* ("the book", free at
doc.rust-lang.org/book) and *Rust by Example*.
