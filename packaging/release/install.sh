#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
prefix="${PREFIX:-$HOME/.local}"
install -Dm755 "$here/usr/bin/poezibrowse" "$prefix/bin/poezibrowse"
install -Dm755 "$here/usr/lib/poezibrowse/lyrebird" "$prefix/lib/poezibrowse/lyrebird"
cp -r "$here/usr/share/." "$prefix/share/"
sed -i "s|^Exec=poezibrowse|Exec=$prefix/bin/poezibrowse|" "$prefix/share/applications/io.github.poezi123.PoeziBrowse.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" 2>/dev/null || true
echo "PoeziBrowse ist installiert: $prefix/bin/poezibrowse"
