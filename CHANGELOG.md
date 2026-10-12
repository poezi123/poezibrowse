# Changelog

## v0.5.0 – built-in Tor bridges / eingebaute Tor-Brücken

**English**

- **Built-in Tor bridges:** obfs4, Snowflake and meek from Tor Browser 15.0.24, chosen from a menu in the settings with the pros and cons of each. The bridge program `lyrebird` from the Tor Project's signed expert bundle now ships with every package – no extra packages needed on any distribution.
- **"New circuit" really gets a new circuit:** PoeziBrowse now gives Tor fresh SOCKS credentials, so WebKit drops its open connections and Tor builds a new path with a different middle and exit relay. Before, the old connections were reused and the IP stayed the same.
- **Circuit view shows the right relays:** a background watcher remembers which circuit each site used; the exit shown now matches the IP the site sees. Relay nicknames are gone – you see country and IP.
- **Copy image** now puts only the image on the clipboard. Before, the image link came along as text and many apps pasted the link instead of the picture.
- **Address bar suggestions are off by default** and can be switched on in the settings.
- Tor settings show the install command for your distribution; Tor restarts by itself when you change the bridge.

**Deutsch**

- **Eingebaute Tor-Brücken:** obfs4, Snowflake und meek aus dem Tor Browser 15.0.24, auswählbar in den Einstellungen, mit Vor- und Nachteilen jeder Brücke. Das Brücken-Programm `lyrebird` aus dem signierten Expert Bundle des Tor-Projekts ist jetzt in jedem Paket – auf keiner Distribution braucht es Zusatzpakete.
- **„Neue Verbindung“ holt wirklich eine neue:** PoeziBrowse gibt Tor neue SOCKS-Zugangsdaten, dadurch verwirft WebKit seine offenen Verbindungen und Tor baut einen neuen Weg mit anderer Mitte und anderem Ausgang. Vorher wurden die alten Verbindungen weiterbenutzt und die IP blieb gleich.
- **Die Anzeige zeigt die richtigen Knoten:** Ein Wächter im Hintergrund merkt sich, über welche Verbindung jede Seite lief; der angezeigte Ausgang passt jetzt zur IP, die die Seite sieht. Statt der Spitznamen der Knoten siehst du Land und IP.
- **Bild kopieren** legt jetzt nur das Bild in die Zwischenablage. Vorher kam der Bildlink als Text mit, und viele Programme haben den Link statt des Bildes eingefügt.
- **Vorschläge in der Adressleiste sind standardmäßig aus** und lassen sich in den Einstellungen einschalten.
- Die Tor-Einstellungen zeigen den Installationsbefehl für deine Distribution; Tor startet neu, wenn du die Brücke wechselst.

## v0.4.2 – no more tearing on NVIDIA / keine Bildfehler mehr mit NVIDIA

**English**

- **Fixed a horizontal line while scrolling on NVIDIA under Wayland:** PoeziBrowse now shows web pages without WebKit's DMA-BUF renderer on NVIDIA + Wayland, which showed half-drawn frames. Set `WEBKIT_DISABLE_DMABUF_RENDERER=0` to get the old behaviour. `--diagnose` shows which mode is active.
- Removed the DNS prefetching switch – it does nothing in current WebKitGTK and only printed a warning.

**Deutsch**

- **Waagerechter Strich beim Scrollen mit NVIDIA unter Wayland behoben:** PoeziBrowse zeigt Webseiten auf NVIDIA + Wayland jetzt ohne WebKits DMA-BUF-Renderer an, der halb gezeichnete Bilder angezeigt hat. Mit `WEBKIT_DISABLE_DMABUF_RENDERER=0` gibt es das alte Verhalten. `--diagnose` zeigt, welcher Modus aktiv ist.
- DNS-Vorabauflösung entfernt – sie tut in aktuellem WebKitGTK nichts und hat nur eine Warnung ausgegeben.

## v0.4.1 – stability on all distributions / Stabilität auf allen Distributionen

**English**

- **Fixed crashes on CachyOS and other minimal installs:** WebKit's web process crashes on every page with video or audio when GStreamer's `autoaudiosink` (gst-plugins-good) is missing. The packages now depend on gst-plugins-good, gst-plugins-base, gst-libav and glib-networking, and PoeziBrowse checks on start and shows the install command for your distribution.
- **Crashes stay contained:** only tabs of the same website share a web process, so one crashing page no longer takes other tabs with it. A crashed tab reloads once by itself, otherwise it shows what happened and a reload button.
- **The main window no longer crashes** when a web process dies (the crash handler now runs after WebKit's signal has finished).
- **Master password: new setting "remember"** – 7 days (TPM), until PoeziBrowse is closed, or never: the password manager stays locked and asks for the KeePassXC master password every time.
- Own fallback icons, so the interface works with every icon theme (e.g. Adwaita 50 without `text-html-symbolic`).
- `poezibrowse --diagnose` shows WebKitGTK version, sandbox, TLS, missing video plugins and the matching install command; crashes are logged in `~/.cache/poezibrowse/crashes.log`.
- **PDF viewer:** PDFs open directly in a tab (PDF.js built into WebKitGTK) instead of being downloaded; PoeziBrowse can be chosen to open PDF files.
- **Broken font configuration is detected:** WebKit crashes on many pages (e.g. Amazon) when fontconfig has no default font. PoeziBrowse checks this on start and shows the repair command.
- **Smoother:** the RAM display no longer reads `smaps_rollup`, which locked each web process's memory for up to 14 ms every 3 seconds; the back/forward cache is always on, so going back is instant.
- Tested on Garuda, CachyOS, Ubuntu 24.04, Debian 13, Fedora 44 and openSUSE Tumbleweed.

**Deutsch**

- **Abstürze auf CachyOS und anderen schlanken Installationen behoben:** Fehlt GStreamers `autoaudiosink` (gst-plugins-good), stürzt WebKits Web-Prozess auf jeder Seite mit Video oder Ton ab. Die Pakete hängen jetzt von gst-plugins-good, gst-plugins-base, gst-libav und glib-networking ab, und PoeziBrowse prüft beim Start und zeigt den Installationsbefehl für deine Distribution.
- **Abstürze bleiben eingegrenzt:** Nur Tabs derselben Website teilen sich einen Web-Prozess, ein abstürzender Tab reißt keine anderen mehr mit. Ein abgestürzter Tab lädt sich einmal selbst neu, sonst zeigt er, was passiert ist, und einen Neu-laden-Knopf.
- **Das Hauptfenster stürzt nicht mehr ab**, wenn ein Web-Prozess stirbt.
- **Master-Passwort: neue Einstellung „merken“** – 7 Tage (TPM), bis PoeziBrowse geschlossen wird, oder nie: der Passwortmanager bleibt gesperrt und fragt jedes Mal nach dem KeePassXC-Master-Passwort.
- Eigene Ersatz-Symbole, damit die Oberfläche mit jedem Icon-Theme funktioniert (z. B. Adwaita 50 ohne `text-html-symbolic`).
- `poezibrowse --diagnose` zeigt WebKitGTK-Version, Sandbox, TLS, fehlende Video-Plugins und den passenden Installationsbefehl; Abstürze landen in `~/.cache/poezibrowse/crashes.log`.
- **PDF-Betrachter:** PDFs öffnen sich direkt im Tab (in WebKitGTK eingebautes PDF.js), statt heruntergeladen zu werden; PoeziBrowse kann zum Öffnen von PDF-Dateien gewählt werden.
- **Kaputte Schrift-Konfiguration wird erkannt:** Findet fontconfig keine Standardschrift, stürzt WebKit auf vielen Seiten ab (z. B. Amazon). PoeziBrowse prüft das beim Start und zeigt den Reparaturbefehl.
- **Flüssiger:** Die RAM-Anzeige liest nicht mehr `smaps_rollup`, das alle 3 Sekunden den Speicher jedes Web-Prozesses für bis zu 14 ms gesperrt hat; der Zurück/Vor-Cache ist immer an, Zurück geht sofort.
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
