# Passwortmanager

PoeziBrowse kann eine **KeePassXC-Datenbank** (`.kdbx`) nutzen oder einen **eigenen Passwortmanager** anlegen (ebenfalls `.kdbx`, also mit KeePassXC kompatibel). Beim ersten Start fragt PoeziBrowse, was du möchtest; ändern kannst du es unter *Einstellungen → Passwörter*.

## So funktioniert es

- **Ausfüllen nur nach Klick.** Hat eine Seite ein Login-Formular und gibt es dafür Einträge, leuchtet der Schlüssel oben rechts. Ein Klick zeigt die passenden Logins, ein weiterer Klick füllt aus.
- **Speichern nur nach „Ja“.** Nach einem Login fragt eine Leiste, ob die Zugangsdaten gespeichert (oder ein geändertes Passwort aktualisiert) werden sollen. Geschrieben wird mit `keepassxc-cli`, also mit KeePassXC selbst – PoeziBrowse verändert die Datei nie direkt. Neue Einträge landen in der Gruppe `PoeziBrowse`.
- **Passwort-Generator.** Auf Registrierungsseiten erzeugt „Sicheres Passwort erzeugen“ ein Passwort mit dem Generator von KeePassXC (20 Zeichen, alle Zeichengruppen) und trägt es ein; nach dem Absenden wird gefragt, ob es gespeichert werden soll.
- **Manager-Ansicht** (Schlüssel-Symbol in der Seitenleiste): alle Einträge mit Titel, Benutzer und Adresse. Passwörter werden nie angezeigt, nur kopiert; die Zwischenablage wird nach 30 Sekunden geleert.

## Sicherheit

- Das Master-Passwort wird in einem eigenen Dialog eingegeben, nie auf einer Webseite.
- Gespeichert wird es mit `systemd-creds --user`: verschlüsselt mit dem **TPM-Chip** des PCs und an dein Benutzerkonto gebunden, mit eingebautem Ablaufdatum nach **7 Tagen**. Auf einem anderen PC oder für einen anderen Benutzer ist die Datei wertlos. Ohne TPM wird der Schlüsselbund (KWallet) verwendet, ohne beides nur der Arbeitsspeicher.
- An `keepassxc-cli` werden Passwörter nur über eine Pipe übergeben, nie als Programmargument.
- Die Datenbank wird nur geöffnet, wenn sie gebraucht wird. Damit PoeziBrowse weiß, welche Seiten Einträge haben, ohne die Datenbank zu öffnen, gibt es einen Index aus HMAC-SHA256-Hashes der Domains; der Schlüssel dafür liegt ebenfalls im TPM-Speicher. Der Index verrät nicht, wo du Konten hast.
- Das Formular-Skript läuft in einer abgeschotteten JavaScript-Welt: Webseiten können es nicht sehen, nicht aufrufen und keine Nachrichten an den Passwortmanager vortäuschen. Es reagiert nur auf echte Klicks und Tasten.
- Ausgefüllt wird nur auf HTTPS-Seiten, nur im Hauptdokument und nur, wenn die Domain zum Eintrag passt (oder eine Subdomain davon ist).
- In Tor-Tabs ist der Passwortmanager aus.
- „Sperren“ löscht das gespeicherte Master-Passwort sofort.
