<p align="center">
  <img src="docs/icon.png" width="128" alt="PoeziBrowse logo">
</p>

<h1 align="center">PoeziBrowse</h1>

<p align="center">
  <b>A lean web browser for Linux, written in Rust.</b><br>
  Sleeping tabs, a built-in ad and tracker blocker, private and Tor tabs, KeePassXC integration – and no telemetry.
</p>

<p align="center">
  <a href="https://github.com/poezi123/poezibrowse/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/poezi123/poezibrowse?label=download&color=0E9C92"></a>
  <a href="https://github.com/poezi123/poezibrowse/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/poezi123/poezibrowse/total?color=7C42DE"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux-222222">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-GPL--3.0-555555"></a>
</p>

<p align="center">
  <a href="#download">Download</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#features">Features</a> ·
  <a href="#memory">Memory</a> ·
  <a href="#keyboard-shortcuts">Shortcuts</a> ·
  <a href="#faq">FAQ</a> ·
  <a href="README.de.md">Deutsch</a>
</p>

<p align="center">
  <img src="docs/screenshots/start.png" alt="PoeziBrowse start page with favorites and wallpaper" width="100%">
</p>

---

## What is PoeziBrowse?

Modern browsers grow with every release. PoeziBrowse goes the other way: a small native GTK interface
written in Rust around the WebKit engine (the engine of Safari and GNOME Web). Tabs you are not using
fall asleep and give their memory back, tabs share processes, and ads and trackers are blocked before
they are even downloaded. There is no account, no sync service, no telemetry and nothing running in the
background.

The interface is in German for now.

## Download

<a id="download"></a>

| System | Package | |
|---|---|---|
| **Arch Linux**, Garuda, CachyOS, Manjaro, EndeavourOS | `poezibrowse-0.4.1-1-x86_64.pkg.tar.zst` | [Download](https://github.com/poezi123/poezibrowse/releases/download/v0.4.1/poezibrowse-0.4.1-1-x86_64.pkg.tar.zst) |
| **Ubuntu 24.04+**, Debian 13, Linux Mint 22 | `poezibrowse_0.4.1_amd64.deb` | [Download](https://github.com/poezi123/poezibrowse/releases/download/v0.4.1/poezibrowse_0.4.1_amd64.deb) |
| **Any other Linux** (Fedora, openSUSE, …) | `poezibrowse-0.4.1-linux-x64.tar.gz` | [Download](https://github.com/poezi123/poezibrowse/releases/download/v0.4.1/poezibrowse-0.4.1-linux-x64.tar.gz) |

All versions and checksums (`SHA256SUMS.txt`) are on the
[releases page](https://github.com/poezi123/poezibrowse/releases).

**Requirements:** Linux on x86_64 with glibc 2.39 or newer, GTK 3 and WebKitGTK 4.1 (2.40 or newer) –
that means Ubuntu 24.04, Debian 13, Fedora 40 or any current Arch-based system.

## Installation

<a id="installation"></a>

**Arch Linux and derivatives**

```sh
sudo pacman -U poezibrowse-0.4.1-1-x86_64.pkg.tar.zst
```

**Ubuntu, Debian, Linux Mint** – double-click the file, or:

```sh
sudo apt install ./poezibrowse_0.4.1_amd64.deb
```

**Any other distribution** – install WebKitGTK 4.1 with your package manager first, then (no root needed):

```sh
tar xzf poezibrowse-0.4.1-linux-x64.tar.gz
cd poezibrowse-0.4.1-linux-x64
./install.sh        # adds PoeziBrowse to your app menu (~/.local)
```

**Optional extras:** `tor` for Tor tabs, `keepassxc` to save logins into a KeePassXC database,
`gst-plugin-dav1d` (Arch) for fast AV1 video, `lyrebird` or `snowflake-client` for Tor bridges.

**Check the download** (optional):

```sh
sha256sum -c SHA256SUMS.txt --ignore-missing
```

## Features

<a id="features"></a>

<p align="center">
  <img src="docs/screenshots/browse.png" alt="Browsing with the history panel" width="100%">
</p>

**Browsing**
- Tabs with drag and drop, the **+** right next to the last tab, mouse wheel switches tabs
- Favorites bar, favorites on the start page, rename them with a right click
- Sidebar with panels for bookmarks, history and downloads
- Address bar suggestions from favorites and history, search with DuckDuckGo, Startpage, Brave, Ecosia, Qwant or Google
- Session restore – restored tabs only load when you open them
- Ctrl + mouse wheel zoom, mouse back/forward buttons, find in page, print, developer tools
- Built-in PDF viewer (PDF.js) – PDFs open in a tab, also from the file manager
- Start page wallpapers: presets or your own picture, with dimming and blur
- Title bar with the animated contour pattern from [PoeziCode](https://github.com/poezi123/poezicode), adjustable frame rate
- Light, dark or system appearance

**Privacy**
- Ad and tracker blocker (EasyList, EasyPrivacy, EasyList Germany), compiled into WebKit's content blocker, updated weekly; switch it off per site with the shield in the address bar
- YouTube ads are removed from the player data and skipped if one gets through
- Private tabs: no history, and their cookies are deleted when the last private tab closes
- No telemetry, no browser name in the user agent, DNS prefetching off, Intelligent Tracking Prevention on

**Tor tabs**

<p align="center">
  <img src="docs/screenshots/tor.png" alt="Tor tab showing the circuit" width="100%">
</p>

- Open a Tor tab with the onion in the sidebar or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>U</kbd>; PoeziBrowse starts its own Tor process
- Bridges: obfs4, webtunnel, meek and Snowflake
- The onion button shows the circuit of the current site – entry, middle and exit relay with country and IP – and gets you a new circuit
- DNS is resolved by Tor; WebRTC and WebGL are off, the time zone is UTC
- Separate Tor favorites; searches go to the DuckDuckGo onion service
- Links opened from a Tor tab stay in Tor tabs

PoeziBrowse hides your IP address, but the Tor Browser protects much better against browser
fingerprinting. If your safety depends on it, use the Tor Browser.

**Passwords**
- Use your **KeePassXC database** or a new PoeziBrowse database (also `.kdbx`)
- Fills in logins only after you click the key, only on HTTPS and only on the matching site
- Asks before saving new or changed logins – and only after the login worked; KeePassXC itself writes the file
- Password generator from KeePassXC for sign-up forms
- The master password is sealed with your PC's TPM (`systemd-creds`) and asked again every 7 days – or, if you choose, kept only until PoeziBrowse closes, or never remembered at all so the manager stays locked. It is never stored in plain text
- Details: [docs/passwortmanager.md](docs/passwortmanager.md)

## Memory

<a id="memory"></a>

Measured on the same machine with a fresh profile (proportional set size of all processes):

| | speedtest.net |
|---|---|
| Firefox | 654–869 MB |
| **PoeziBrowse** | **~480 MB** |

Most of a browser's memory is used by the web pages themselves, so PoeziBrowse saves where it can:

- Background tabs fall asleep after 10 minutes and free their memory completely
- A RAM budget (default 1.5 GB): above it, the least recently used tab goes to sleep immediately
- Up to six tabs of the same website share one web process – a crash never takes other sites with it
- WebKit releases its caches when PoeziBrowse is in the background
- Filter lists are compiled in a short-lived helper process

## Keyboard shortcuts

<a id="keyboard-shortcuts"></a>

| Shortcut | Action |
|---|---|
| <kbd>Ctrl</kbd>+<kbd>T</kbd> / <kbd>Ctrl</kbd>+<kbd>W</kbd> | New tab / close tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | Reopen closed tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> | Private tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>U</kbd> | Tor tab |
| <kbd>Ctrl</kbd>+<kbd>L</kbd> | Address bar |
| <kbd>Ctrl</kbd>+<kbd>D</kbd> | Add to favorites |
| <kbd>Ctrl</kbd>+<kbd>H</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Y</kbd> | History / bookmarks / downloads panel |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Find in page |
| <kbd>Ctrl</kbd>+<kbd>Tab</kbd>, <kbd>Ctrl</kbd>+<kbd>1</kbd>…<kbd>9</kbd> | Switch tabs |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Page Up/Down</kbd> | Move tab |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Zoom |
| <kbd>F9</kbd> | Show or hide the sidebar |
| <kbd>F11</kbd> / <kbd>F12</kbd> | Full screen / developer tools |

## FAQ

<a id="faq"></a>

**Why WebKit and not a browser engine written from scratch?**
Writing an engine that handles today's web is a project of many years and hundreds of people.
WebKit is mature, fast, and lighter than Chromium. Support for [Servo](https://servo.org), the browser
engine written in Rust, is planned as a second engine – see the
[issue](https://github.com/poezi123/poezibrowse/issues/1).

**Some WebM videos do not play.**
WebKitGTK 2.52 fails to play VP9 videos embedded directly as WebM files – the same happens in
WebKit's own MiniBrowser. PoeziBrowse tells pages that VP9 is unavailable and switches such videos to
the page's MP4 version when there is one. YouTube is not affected.

**Pages crash or do not load (e.g. on CachyOS).**
Run `poezibrowse --diagnose`. WebKit's web process crashes on pages with video or audio when the
GStreamer plugins are missing; the output names them and the install command for your distribution
(Arch/CachyOS: `sudo pacman -S --needed gst-plugins-good gst-plugins-base gst-libav`). Crashes are
logged in `~/.cache/poezibrowse/crashes.log`.

**Is Tor mode as safe as the Tor Browser?**
No. It hides your IP address and resolves names through Tor, but it does not have the Tor Browser's
protection against fingerprinting.

**Where is my data?**
`~/.local/share/poezibrowse` (history, favorites, session, website data) and
`~/.config/poezibrowse/config.toml` (settings). Both are only readable by your user.

**How do I build it myself?**

```sh
sudo pacman -S rust webkit2gtk-4.1 gtk3 sqlite      # Arch
cargo build --release
./target/release/poezibrowse
```

## License

<a id="license"></a>

PoeziBrowse is free software under the [GNU General Public License v3.0 or later](LICENSE).
