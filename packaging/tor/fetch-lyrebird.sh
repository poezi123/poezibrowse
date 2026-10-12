#!/usr/bin/env bash
set -euo pipefail

version=15.0.24
sha256=8e012ec6815d7899cb64011582e2dade88e74119c6661068a2a3252de0ccd7f2
name="tor-expert-bundle-linux-x86_64-$version.tar.gz"
url="https://dist.torproject.org/torbrowser/$version/$name"
dest="${1:?Zielordner fehlt – packaging/tor/fetch-lyrebird.sh <zielordner>}"
cache="${XDG_CACHE_HOME:-$HOME/.cache}/poezibrowse-build"

mkdir -p "$cache"
if ! echo "$sha256  $cache/$name" | sha256sum -c --status 2>/dev/null; then
  curl -fL --proto '=https' -o "$cache/$name.part" "$url"
  mv "$cache/$name.part" "$cache/$name"
fi
echo "$sha256  $cache/$name" | sha256sum -c --quiet

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tar -C "$work" -xzf "$cache/$name" tor/pluggable_transports/lyrebird docs/lyrebird.txt
install -Dm755 "$work/tor/pluggable_transports/lyrebird" "$dest/lyrebird"
install -Dm644 "$work/docs/lyrebird.txt" "$dest/LICENSE.lyrebird"
