# Desktop shortcut and test menu

One shortcut opens a menu to run the Rust remaster **and** the development tasks (run the tests,
rebuild after editing, rebuild the index, open the folder or the guide). It rebuilds when needed, so
after you edit code you just pick an option again.

**Windows** (needs [Rust](https://rustup.rs), installed once):
1. Double-click `launch\windows\create-desktop-shortcut.bat` (once).
2. Use the new **TBJP Oracle (Rust)** shortcut on your desktop.

**macOS / Linux:** run `launch/unix/create-desktop-shortcut.sh` once, then use the launcher it creates.

The search index is created in `data/index/` (ignored by git) the first time you open the web app.
