# Changelog

## v0.4.1 – stability on all distributions / Stabilität auf allen Distributionen

**English**

- **Fixed crashes on CachyOS and other minimal installs:** WebKit's web process crashes on every page with video or audio when GStreamer's `autoaudiosink` (gst-plugins-good) is missing. The packages now depend on gst-plugins-good, gst-plugins-base, gst-libav and glib-networking, and PoeziBrowse checks on start and shows the install command for your distribution.
- **Crashes stay contained:** only tabs of the same website share a web process, so one crashing page no longer takes other tabs with it. A crashed tab reloads once by itself, otherwise it shows what happened and a reload button.
- **The main window no longer crashes** when a web process dies (the crash handler now runs after WebKit's signal has finished).
- **Master password: new setting "remember"** – 7 days (TPM), until PoeziBrowse is closed, or never: the password manager stays locked and asks for the KeePassXC master password every time.
- Own fallback icons, so the interface works with every icon theme (e.g. Adwaita 50 without `text-html-symbolic`).
- `poezibrowse --diagnose` shows WebKitGTK version, sandbox, TLS, missing video plugins and the matching install command; crashes are logged in `~/.cache/poezibrowse/crashes.log`.
- Tested on Garuda, CachyOS, Ubuntu 24.04, Debian 13, Fedora 44 and openSUSE Tumbleweed.

**Deutsch**

- **Abstürze auf CachyOS und anderen schlanken Installationen behoben:** Fehlt GStreamers `autoaudiosink` (gst-plugins-good), stürzt WebKits Web-Prozess auf jeder Seite mit Video oder Ton ab. Die Pakete hängen jetzt von gst-plugins-good, gst-plugins-base, gst-libav und glib-networking ab, und PoeziBrowse prüft beim Start und zeigt den Installationsbefehl für deine Distribution.
- **Abstürze bleiben eingegrenzt:** Nur Tabs derselben Website teilen sich einen Web-Prozess, ein abstürzender Tab reißt keine anderen mehr mit. Ein abgestürzter Tab lädt sich einmal selbst neu, sonst zeigt er, was passiert ist, und einen Neu-laden-Knopf.
- **Das Hauptfenster stürzt nicht mehr ab**, wenn ein Web-Prozess stirbt.
- **Master-Passwort: neue Einstellung „merken“** – 7 Tage (TPM), bis PoeziBrowse geschlossen wird, oder nie: der Passwortmanager bleibt gesperrt und fragt jedes Mal nach dem KeePassXC-Master-Passwort.
- Eigene Ersatz-Symbole, damit die Oberfläche mit jedem Icon-Theme funktioniert (z. B. Adwaita 50 ohne `text-html-symbolic`).
- `poezibrowse --diagnose` zeigt WebKitGTK-Version, Sandbox, TLS, fehlende Video-Plugins und den passenden Installationsbefehl; Abstürze landen in `~/.cache/poezibrowse/crashes.log`.
- Getestet auf Garuda, CachyOS, Ubuntu 24.04, Debian 13, Fedora 44 und openSUSE Tumbleweed.

## v0.4.0 – first public release / erste öffentliche Version

**English**

- **Lean WebKit browser in Rust** with a native GTK interface in the dark PoeziCode look and the animated contour title bar.
- **Memory saving:** background tabs fall asleep after 10 minutes, a RAM budget puts the least recently used tab to sleep, up to six tabs share one web process, caches are released when PoeziBrowse is in the background.
- **Ad and tracker blocker** (EasyList, EasyPrivacy, EasyList Germany) in WebKit's content blocker, per-site switch, YouTube ad removal.
- **Private tabs** and **Tor tabs** with bridges (obfs4, webtunnel, meek, Snowflake), circuit view with entry, middle and exit relay, separate Tor favorites.
- **Password manager** for KeePassXC databases or an own `.kdbx` database: fill after a click, save after a yes, KeePassXC password generator, master password sealed with the TPM and asked again every 7 days.
- **Favorites bar**, start page with favorites and wallpapers, sidebar with bookmarks, history and downloads.
- Tabs with drag and drop, session restore, Ctrl + mouse wheel zoom, mouse back/forward buttons, find in page, print, developer tools.
- Workaround for WebKitGTK failing to play VP9 WebM videos.

**Deutsch**

- **Schlanker WebKit-Browser in Rust** mit nativer GTK-Oberfläche im dunklen PoeziCode-Stil und wandernder Höhenlinien-Fensterleiste.
- **RAM sparen:** Hintergrund-Tabs schlafen nach 10 Minuten ein, ein RAM-Budget legt den am längsten unbenutzten Tab schlafen, bis zu sechs Tabs teilen sich einen Web-Prozess, Caches werden freigegeben, wenn PoeziBrowse im Hintergrund ist.
- **Werbe- und Trackerblocker** (EasyList, EasyPrivacy, EasyList Germany) im Content-Blocker von WebKit, pro Seite abschaltbar, YouTube-Werbung wird entfernt.
- **Private Tabs** und **Tor-Tabs** mit Brücken (obfs4, webtunnel, meek, Snowflake), Anzeige von Eingangs-, Mittel- und Ausgangsknoten, eigene Tor-Favoriten.
- **Passwortmanager** für KeePassXC-Datenbanken oder eine eigene `.kdbx`-Datenbank: Ausfüllen nach Klick, Speichern nach „Ja“, Passwort-Generator von KeePassXC, Master-Passwort mit dem TPM-Chip verschlüsselt und alle 7 Tage neu abgefragt.
- **Favoritenleiste**, Startseite mit Favoriten und Hintergründen, Seitenleiste mit Lesezeichen, Verlauf und Downloads.
- Tabs mit Drag & Drop, Sitzung wiederherstellen, Zoom mit Strg + Mausrad, Daumentasten der Maus, Suchen auf der Seite, Drucken, Entwicklerwerkzeuge.
- Workaround dafür, dass WebKitGTK VP9-WebM-Videos nicht abspielt.
