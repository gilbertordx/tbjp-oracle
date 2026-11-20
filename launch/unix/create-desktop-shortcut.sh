#!/usr/bin/env bash
# Creates a desktop launcher for the TBJP Oracle menu (macOS: .command file, Linux: .desktop file).
set -e
here="$(cd "$(dirname "$0")" && pwd)"
menu="$here/tbjp-oracle-menu.sh"
chmod +x "$menu"
desktop="${XDG_DESKTOP_DIR:-$HOME/Desktop}"
mkdir -p "$desktop"
name="TBJP Oracle (Rust)"
if [ "$(uname)" = "Darwin" ]; then
  file="$desktop/$name.command"
  printf '#!/usr/bin/env bash\nexec "%s"\n' "$menu" > "$file"
  chmod +x "$file"
else
  file="$desktop/$name.desktop"
  cat > "$file" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$name
Comment=TBJP Oracle - Rust remaster: run it, test it, rebuild it
Exec=bash "$menu"
Terminal=true
DESKTOP
  chmod +x "$file"
  command -v gio >/dev/null 2>&1 && gio set "$file" metadata::trusted true 2>/dev/null || true
fi
echo "Created: $file"
