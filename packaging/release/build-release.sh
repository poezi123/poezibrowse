#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
target="${1:?Zielordner fehlt – packaging/release/build-release.sh <zielordner>}"
version="$(sed -nE 's/^version = "([0-9.]+)"/\1/p' "$root/Cargo.toml" | head -1)"
appid=io.github.poezi123.PoeziBrowse
binary="$root/target/release/poezibrowse"

(cd "$root" && cargo build --release --locked)

mkdir -p "$target"
target="$(cd "$target" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

stage() {
  local dest="$1"
  install -Dm755 "$binary" "$dest/usr/bin/poezibrowse"
  install -Dm644 "$root/packaging/linux/$appid.desktop" "$dest/usr/share/applications/$appid.desktop"
  for size in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$root/packaging/linux/icons/$size.png" "$dest/usr/share/icons/hicolor/${size}x${size}/apps/$appid.png"
  done
  install -Dm644 "$root/packaging/linux/icon.svg" "$dest/usr/share/icons/hicolor/scalable/apps/$appid.svg"
}

deb="$work/deb"
stage "$deb"
install -Dm644 "$root/LICENSE" "$deb/usr/share/doc/poezibrowse/copyright"
find "$deb" -type d -exec chmod 755 {} +
install -d "$deb/DEBIAN"
cat > "$deb/DEBIAN/control" <<CONTROL
Package: poezibrowse
Version: $version
Architecture: amd64
Maintainer: poezi123 <poezi123@users.noreply.github.com>
Installed-Size: $(du -sk --exclude=DEBIAN "$deb" | cut -f1)
Depends: libc6 (>= 2.39), libwebkit2gtk-4.1-0 (>= 2.40), libgtk-3-0t64 | libgtk-3-0, libsqlite3-0, libglib2.0-0t64 | libglib2.0-0, glib-networking, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good, gstreamer1.0-libav
Recommends: bubblewrap, xdg-dbus-proxy, gstreamer1.0-plugins-bad
Suggests: tor, keepassxc, obfs4proxy
Section: web
Priority: optional
Homepage: https://github.com/poezi123/poezibrowse
Description: Schlanker Webbrowser für Linux, geschrieben in Rust
 PoeziBrowse spart Arbeitsspeicher mit schlafenden Tabs, geteilten Prozessen
 und einem eingebauten Werbe- und Trackerblocker. Mit privaten Tabs, Tor-Tabs
 und KeePassXC-Anbindung.
CONTROL
dpkg-deb --root-owner-group --build "$deb" "$target/poezibrowse_${version}_amd64.deb" >/dev/null

tarroot="$work/poezibrowse-$version-linux-x64"
stage "$tarroot"
install -Dm644 "$root/LICENSE" "$tarroot/LICENSE"
install -Dm755 "$root/packaging/release/install.sh" "$tarroot/install.sh"
install -Dm755 "$root/packaging/release/uninstall.sh" "$tarroot/uninstall.sh"
tar -C "$work" --owner=0 --group=0 -czf "$target/poezibrowse-$version-linux-x64.tar.gz" "poezibrowse-$version-linux-x64"

for pkg in "$root"/packaging/arch/poezibrowse-"$version"-*.pkg.tar.zst; do
  [[ -f "$pkg" ]] && cp "$pkg" "$target/"
done

(cd "$target" && sha256sum poezibrowse* > SHA256SUMS.txt)
echo "Fertig: $target"
ls -la "$target"
