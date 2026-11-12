#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
out="${1:-$root/release/chromium-bundle}"
version="$(sed -nE 's/^version = "([0-9.]+)"/\1/p' "$root/Cargo.toml" | head -1)"
binary="$root/target/release/poezibrowse"

if [ ! -x "$binary" ]; then
  echo "Release-Binary fehlt. Zuerst: cargo build --release --features cef" >&2
  exit 1
fi

cefdir="$(find "$root/target/release/build" -maxdepth 3 -type d -name cef_linux_x86_64 | head -1)"
if [ -z "$cefdir" ]; then
  echo "CEF-Distribution nicht gefunden unter target/release/build" >&2
  exit 1
fi

dest="$out/poezibrowse-$version-chromium-x86_64"
rm -rf "$dest"
mkdir -p "$dest/locales"

install -Dm755 "$binary" "$dest/poezibrowse"
install -Dm755 "$cefdir/libcef.so" "$dest/libcef.so"
install -Dm755 "$cefdir/libvk_swiftshader.so" "$dest/libvk_swiftshader.so"
install -Dm755 "$cefdir/libEGL.so" "$dest/libEGL.so" 2>/dev/null || true
install -Dm755 "$cefdir/libGLESv2.so" "$dest/libGLESv2.so" 2>/dev/null || true
install -Dm755 "$cefdir/libvulkan.so.1" "$dest/libvulkan.so.1" 2>/dev/null || true
install -Dm4755 "$cefdir/chrome-sandbox" "$dest/chrome-sandbox"
for f in icudtl.dat v8_context_snapshot.bin snapshot_blob.bin chrome_100_percent.pak chrome_200_percent.pak resources.pak vk_swiftshader_icd.json; do
  [ -f "$cefdir/$f" ] && install -Dm644 "$cefdir/$f" "$dest/$f"
done
cp -a "$cefdir/locales/." "$dest/locales/" 2>/dev/null || true

cat > "$dest/poezibrowse-chromium" <<'LAUNCH'
#!/usr/bin/env bash
here="$(cd "$(dirname "$0")" && pwd)"
export GDK_BACKEND=x11
exec "$here/poezibrowse" "$@"
LAUNCH
chmod 755 "$dest/poezibrowse-chromium"

size="$(du -sh "$dest" | cut -f1)"
echo "Bündel fertig ($size): $dest"
echo "Starten: $dest/poezibrowse-chromium"
echo "Dann Einstellungen -> Engine für neue Tabs -> Chromium, Neustart."
