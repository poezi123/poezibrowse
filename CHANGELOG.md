# Changelog

## v0.8.0 – security tools, certificates, better settings / Sicherheits-Werkzeuge, Zertifikate, bessere Einstellungen

**English**

- **Invalid certificates can now be allowed:** the warning page has an "Allow anyway" button for your own devices (router, NAS …). Allowed certificates are listed under Settings → Certificates, where you can block them again.
- **PoeziIntercept** (sidebar): a built-in request tool for authorized security testing, in an adjustable split next to the page – a request log, a repeater (edit method/URL/headers/body, resend via libsoup, raw response + missing-security-header check), an intruder (fuzz a marked `§` position with built-in OWASP payload lists: XSS, SQLi, path traversal, command injection, SSTI) and an intercept mode (hold page loads/form submits, then forward/edit/drop; closing wipes the captured log).
- **Terminal** (sidebar): a real terminal in the same adjustable split, using the system vte, with Ctrl+V paste and Ctrl+Shift+C copy.
- **Split view:** drag a tab onto the middle of another to show both side by side in an adjustable split; end it with the "Teilen beenden" button, by dragging a tab out, closing one, or switching tab.
- **Favourites** can be reordered by dragging, in the bar and in a new overflow dropdown; what doesn't fit moves into the dropdown.
- **Video mode** setting (Auto/Compatible/Software/GPU) against green or flickering video on NVIDIA; more animations that follow the browser's own "Animations" setting.
- **Downloads:** your system's save dialog now appears, plus optional folder rules (e.g. mp3 files from a site straight into a chosen folder).
- **Settings** got a search box and chapter-navigation buttons. New security options (their own chapter, not a separate page): HTTPS-only with exceptions, WebRTC off, local-network protection (SSRF-hardened against decimal/octal/short IP forms), JavaScript JIT off.
- Hide favourites on the new-tab page; animations can be tuned or turned off.
- **Efficiency:** request logging only runs while PoeziIntercept is open; the title-bar animation pauses when the window is unfocused; the RAM readout polls lighter and pauses in the background.
- The package now depends on vte (terminal).

**Deutsch**

- **Ungültige Zertifikate lassen sich jetzt zulassen:** Die Warnseite hat einen Button „Trotzdem zulassen“ für eigene Geräte (Router, NAS …). Zugelassene Zertifikate stehen unter Einstellungen → Zertifikate und lassen sich dort wieder blockieren.
- **PoeziIntercept** (Seitenleiste): ein eingebautes Request-Werkzeug für autorisierte Sicherheitstests, in einem anpassbaren Split neben der Seite – ein Request-Verlauf, ein Repeater (Methode/URL/Header/Body bearbeiten, über libsoup senden, rohe Antwort mit Prüfung fehlender Sicherheits-Header) und ein Intruder (fuzzt eine mit `§` markierte Stelle über eingebaute OWASP-Listen: XSS, SQLi, Path Traversal, Command Injection, SSTI).
- **Terminal** (Seitenleiste): ein echtes Terminal im selben anpassbaren Split, über das System-vte, mit Strg+V einfügen und Strg+Umschalt+C kopieren. Der Intercept-Modus hält Anfragen an und lässt dich weiterleiten/bearbeiten/verwerfen.
- **Geteilte Ansicht:** einen Tab auf die Mitte eines anderen ziehen → beide nebeneinander, Trennlinie verschiebbar; beenden per „Teilen beenden", Tab herausziehen, einen schließen oder anderen Tab anklicken.
- **Favoriten** lassen sich per Ziehen sortieren, in der Leiste und in einem neuen Dropdown; was nicht in die Leiste passt, wandert ins Dropdown.
- **Video-Darstellung** (Automatisch/Kompatibel/Software/GPU) gegen grüne oder flackernde Videos auf NVIDIA; mehr Animationen, die der browser-eigenen „Animationen"-Einstellung folgen.
- **Downloads:** Der Speichern-Dialog des Systems erscheint jetzt, dazu optionale Ordner-Regeln (z. B. MP3s von einer Seite direkt in einen gewählten Ordner).
- **Einstellungen** haben eine Suche und Kapitel-Buttons bekommen. Neue Sicherheits-Optionen (eigenes Kapitel, keine eigene Seite): Nur HTTPS mit Ausnahmen, WebRTC aus, Schutz des lokalen Netzwerks, JavaScript-JIT aus.
- Favoriten auf der Startseite ausblenden; Animationen einstellbar oder abschaltbar.
- **Effizienz:** Request-Mitschnitt läuft nur bei offenem PoeziIntercept; die RAM-Anzeige fragt alle 5 s statt 3 s ab und scannt nicht mehr jedes Mal ganz /proc (die Prozessliste wird zwischengespeichert, voller Scan nur noch etwa einmal pro Minute).
- Das Paket hängt jetzt von vte (Terminal) ab.

## v0.7.1 – no more freezes / keine Hänger mehr

**English**

- **Fixed the browser freezing for a moment again and again** (no clicks, no scrolling, cursor stuck): since 0.7.0 the login key was decrypted with the TPM on the main thread every time a site changed a cookie – about 0.6 s each time. The key is now decrypted once in the background at startup, encryption and writing run in a background thread, and saving happens at most every 30 seconds.
- Permission questions (camera, location, notifications …) no longer block the window – a hidden dialog on another monitor could freeze it under tiling window managers like Hyprland.
- **Green videos on NVIDIA:** video frames no longer go through WebKit's DMA-BUF video sink on NVIDIA.
- **Less CPU for videos:** without the fast dav1d decoder, AV1 is no longer offered to sites, so YouTube uses VP9 or H.264 instead of the slow aom decoder. The Arch package now depends on gst-plugin-dav1d.
- Ad-block rule lists are only swapped when they actually change, not on every navigation.

**Deutsch**

- **Behoben: Der Browser fror immer wieder kurz ein** (kein Klick, kein Scrollen, Cursor reagiert nicht): Seit 0.7.0 wurde der Login-Schlüssel bei jeder Cookie-Änderung einer Seite im Hauptthread mit dem TPM-Chip entschlüsselt – jedes Mal rund 0,6 s. Jetzt wird er einmal beim Start im Hintergrund entschlüsselt, Verschlüsseln und Schreiben laufen in einem Hintergrund-Thread, gespeichert wird höchstens alle 30 Sekunden.
- Berechtigungsfragen (Kamera, Standort, Benachrichtigungen …) blockieren das Fenster nicht mehr – ein versteckter Dialog auf einem anderen Monitor konnte es unter Tiling-Fenstermanagern wie Hyprland einfrieren.
- **Grüne Videos auf NVIDIA:** Videobilder laufen auf NVIDIA nicht mehr über WebKits DMA-BUF-Videoausgabe.
- **Weniger CPU bei Videos:** Ohne den schnellen dav1d-Decoder wird Seiten kein AV1 mehr angeboten, YouTube nimmt dann VP9 oder H.264 statt des langsamen aom-Decoders. Das Arch-Paket hängt jetzt von gst-plugin-dav1d ab.
- Werbeblocker-Listen werden nur noch ausgetauscht, wenn sich wirklich etwas ändert, nicht bei jedem Seitenwechsel.

## v0.7.0 – faster, safer, logins remembered / schneller, sicherer, Logins bleiben

**English**

- **Logins are remembered – encrypted.** WebKit never kept cookies on disk, so you had to sign in again after every restart (e.g. GitHub). PoeziBrowse now saves them itself, encrypted with ChaCha20-Poly1305; the key is sealed with the TPM (systemd-creds) or the system keyring. Nothing is stored in plain text.
- **About 10 % faster page loads** (measured three times on 12 page loads): leaner cosmetic ad-block rules (only site-specific ones) and the bigger cache are now the default.
- **New tabs open about three times faster** (75 instead of 237 ms): a web process is kept warm.
- **Malware and phishing protection** with URLhaus and the Phishing Filter, checked before any request leaves the browser; "open anyway" for the session.
- **Warning when WebKitGTK is outdated**, with the update command for your distribution (checked at most once a day).
- **Cookie policy** (third-party cookies blocked by default) and **delete on exit** for cookies, cache and history.
- **Site permissions** are remembered per site and can be changed or removed in the settings.
- The about page lists the last page load times.
- Every new option can be switched in the settings.

**Deutsch**

- **Logins werden gemerkt – verschlüsselt.** WebKit hat Cookies nie dauerhaft gespeichert, deshalb musstest du dich nach jedem Neustart neu anmelden (z. B. bei GitHub). PoeziBrowse speichert sie jetzt selbst, verschlüsselt mit ChaCha20-Poly1305; der Schlüssel ist mit dem TPM-Chip (systemd-creds) oder dem Schlüsselbund gesichert. Nichts liegt im Klartext.
- **Seiten laden rund 10 % schneller** (dreimal gemessen mit 12 Seitenaufrufen): schlankere Ausblende-Regeln des Werbeblockers (nur seitenspezifische) und der größere Cache sind jetzt Standard.
- **Neue Tabs öffnen etwa dreimal so schnell** (75 statt 237 ms): ein Web-Prozess wird bereitgehalten.
- **Schutz vor Schad- und Phishing-Seiten** mit URLhaus und Phishing Filter, geprüft bevor eine Anfrage den Browser verlässt; „Trotzdem öffnen“ für die Sitzung.
- **Warnung bei veraltetem WebKitGTK** mit dem Update-Befehl für deine Distribution (höchstens einmal am Tag geprüft).
- **Cookie-Regel** (Drittanbieter-Cookies standardmäßig blockiert) und **Löschen beim Beenden** für Cookies, Cache und Verlauf.
- **Website-Berechtigungen** werden pro Seite gemerkt und lassen sich in den Einstellungen ändern oder entfernen.
- Die Seite „Über PoeziBrowse“ zeigt die letzten Ladezeiten.
- Jede neue Option ist in den Einstellungen schaltbar.

## v0.6.0 – proxy, fingerprinting protection, user agent / Proxy, Fingerprinting-Schutz, User-Agent

**English**

- **Proxy** for normal and private tabs: system setting, none, or your own HTTP, HTTPS, SOCKS4 or SOCKS5 proxy with exceptions – applied immediately.
- **Fingerprinting protection** (off / standard / strict, standard by default): per-session noise on canvas and audio fingerprints, hidden GPU and CPU core count, screen size without the panel, Global Privacy Control. Strict also disables WebGL, WebRTC, gamepads and speech synthesis and uses UTC and English.
- **User agent** can be changed: Safari (macOS), Chrome or Firefox (Windows) or your own; `navigator.platform` matches.
- **Tor hardening:** HTTPS only (plain HTTP is upgraded or blocked, .onion excepted), stream isolation per destination, external URI schemes (`magnet:`, `mailto:` …) blocked, always strict fingerprinting protection with a Safari user agent.

**Deutsch**

- **Proxy** für normale und private Tabs: wie im System, keiner, oder ein eigener HTTP-, HTTPS-, SOCKS4- oder SOCKS5-Proxy mit Ausnahmen – wirkt sofort.
- **Schutz vor Fingerprinting** (aus / Standard / streng, Standard ist voreingestellt): Canvas- und Audio-Fingerprints werden pro Sitzung verrauscht, Grafikkarte und Prozessorkerne verborgen, Bildschirmgröße ohne Leiste, Global Privacy Control. Streng schaltet zusätzlich WebGL, WebRTC, Gamepads und Sprachausgabe ab und nutzt UTC und Englisch.
- **User-Agent** änderbar: Safari (macOS), Chrome oder Firefox (Windows) oder ein eigener; `navigator.platform` passt dazu.
- **Tor abgesichert:** Nur HTTPS (HTTP wird umgestellt oder blockiert, .onion ausgenommen), eigene Verbindung pro Ziel, fremde Link-Arten (`magnet:`, `mailto:` …) blockiert, immer strenger Fingerprinting-Schutz mit Safari-User-Agent.

## v0.5.1

- **Fixed YouTube and other video pages crashing on NVIDIA under Wayland with WebKitGTK 2.54:** the DMA-BUF workaround from 0.4.2 makes WebKitGTK 2.54 abort as soon as a page needs accelerated compositing. It is now only used with older WebKitGTK versions.
- **YouTube und andere Videoseiten stürzen auf NVIDIA unter Wayland mit WebKitGTK 2.54 nicht mehr ab:** Der DMA-BUF-Workaround aus 0.4.2 lässt WebKitGTK 2.54 abbrechen, sobald eine Seite beschleunigtes Compositing braucht. Er wird nur noch bei älteren WebKitGTK-Versionen benutzt.

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
