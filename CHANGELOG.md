# Changelog

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
