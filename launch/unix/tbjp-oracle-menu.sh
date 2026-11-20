#!/usr/bin/env bash
# Menu for TBJP Oracle (Rust remaster): run it, test it, rebuild it.
cd "$(dirname "$0")/../.." || exit 1
BIN=rust/target/release/tbjp-oracle

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is not installed yet. Install it from https://rustup.rs and run this again."
  read -r -p "Press Enter to close..." _; exit 1
fi

build() {
  echo; echo "=== Building (the first time takes a few minutes) ==="
  cargo build --release --manifest-path rust/Cargo.toml || { echo "The build failed - read the messages above."; return 1; }
}
open_path() { if command -v xdg-open >/dev/null 2>&1; then xdg-open "$1"; else open "$1"; fi; }

build
while true; do
  clear 2>/dev/null
  cat <<'MENU'
============================================================
  TBJP Oracle (ARCHIVE) - Rust remaster
============================================================
  [1] Open the web app              (http://127.0.0.1:8501)
  [2] Rebuild the search index      (from data/raw_forum_data.json)
  [3] Run the tests
  [4] Rebuild after editing code
  [5] Open the project folder
  [6] Open the learning guide
  [7] Quit
MENU
  read -r -p "Choose 1-7: " choice || exit 0   # end of input: leave instead of looping
  case "$choice" in
    1) if [ ! -f data/index/docs.jsonl ]; then echo; echo "=== Building the search index first (about a minute) ==="; "$BIN" ingest || continue; fi
       echo; echo "Web app on http://127.0.0.1:8501  (Ctrl+C to stop)"
       ( sleep 3; open_path http://127.0.0.1:8501 >/dev/null 2>&1 ) &
       "$BIN" serve ;;
    2) "$BIN" ingest ;;
    3) cargo test --release --manifest-path rust/Cargo.toml ;;
    4) build ;;
    5) open_path . ;;
    6) open_path docs/RUST_LEARNING.md ;;
    7) exit 0 ;;
    *) echo "Please type a number from 1 to 7." ;;
  esac
  echo; read -r -p "Press Enter to return to the menu..." _ || exit 0
done
