@echo off
setlocal EnableDelayedExpansion
cd /d "%~dp0..\.."
title TBJP Oracle (Rust) - menu
set "BIN=rust\target\release\tbjp-oracle.exe"

where cargo >nul 2>nul
if errorlevel 1 (
  echo.
  echo Rust is not installed yet.
  echo Install it from https://rustup.rs  ^(it also asks for the Visual Studio C++ Build Tools^),
  echo then double-click this shortcut again.
  pause
  exit /b 1
)

:build
echo.
echo === Building ^(the first time takes a few minutes, later runs are quick^) ===
cargo build --release --manifest-path rust\Cargo.toml
if errorlevel 1 (
  echo.
  echo The build failed - read the messages above.
  pause
  goto menu
)

:menu
cls
echo ============================================================
echo   TBJP Oracle (ARCHIVE) - Rust remaster
echo ============================================================
echo   [1] Open the web app              ^(http://127.0.0.1:8501^)
echo   [2] Rebuild the search index      ^(from data\raw_forum_data.json^)
echo   [3] Run the tests
echo   [4] Rebuild after editing code
echo   [5] Open the project folder
echo   [6] Open the learning guide
echo   [7] Quit
echo.
choice /c 1234567 /n /m "Choose 1-7: "
if errorlevel 7 goto quit
if errorlevel 6 goto guide
if errorlevel 5 goto folder
if errorlevel 4 goto build
if errorlevel 3 goto tests
if errorlevel 2 goto reindex
if errorlevel 1 goto web

:web
if not exist "data\index\docs.jsonl" (
  echo.
  echo === Building the search index first ^(about a minute^) ===
  "%BIN%" ingest
  if errorlevel 1 ( echo Indexing failed. & pause & goto menu )
)
echo.
echo Web app on http://127.0.0.1:8501   ^(press Ctrl+C in this window to stop^)
start "" cmd /c "timeout /t 3 >nul & start http://127.0.0.1:8501"
"%BIN%" serve
pause
goto menu

:reindex
"%BIN%" ingest
pause
goto menu

:tests
cargo test --release --manifest-path rust\Cargo.toml
pause
goto menu

:folder
start "" "%CD%"
goto menu

:guide
start "" "docs\RUST_LEARNING.md"
goto menu

:quit
endlocal
exit /b 0
