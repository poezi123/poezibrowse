#!/usr/bin/env bash
set -euo pipefail
prefix="${PREFIX:-$HOME/.local}"
rm -f "$prefix/bin/poezibrowse" "$prefix/share/applications/io.github.poezi123.PoeziBrowse.desktop"
find "$prefix/share/icons/hicolor" -name "io.github.poezi123.PoeziBrowse.*" -delete 2>/dev/null || true
echo "PoeziBrowse wurde entfernt. Deine Daten liegen weiter in ~/.local/share/poezibrowse und ~/.config/poezibrowse."
