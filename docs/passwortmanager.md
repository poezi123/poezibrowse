# Passwortmanager – Leons Teil 🔑

`src/passwords.rs` ist schon mit dem Browser verdrahtet:

* `setup_page` wird für JEDE neue Seite aufgerufen (genauer: für ihren `UserContentManager`).
  Hier kannst du JavaScript in Seiten einschleusen und Nachrichten von dort empfangen.
* `on_button_clicked` wird aufgerufen, wenn jemand auf den Schlüssel-Button oben rechts klickt.

Sonst musst du nirgends etwas ändern. Wenn du doch mehr brauchst (z. B. einen Menüeintrag),
schau in `src/window.rs` nach `password_button`.

## Vorschlag für den Aufbau

1. **Formulare erkennen**: In `setup_page` ein User-Script einfügen, das auf `submit`-Events
   lauscht, Benutzername + Passwortfeld ausliest und per
   `window.webkit.messageHandlers.poeziPasswords.postMessage({...})` an Rust schickt.
   - `webkit2gtk::UserScript::new(js, UserContentInjectedFrames::AllFrames,
     UserScriptInjectionTime::End, &[], &[])` und `ucm.add_script(&script)`
   - `ucm.register_script_message_handler("poeziPasswords")` und
     `ucm.connect_script_message_received(Some("poeziPasswords"), |ucm, result| { … })`
   - An die Daten kommst du über `result.js_value()` (Crate `javascriptcore-rs`,
     `value.to_json(0)` gibt dir einen JSON-String, den du mit `serde_json` parst).
2. **Sicher speichern**: Passwörter NIE im Klartext auf die Platte schreiben. Am einfachsten
   ist der Secret Service von Linux (GNOME Keyring / KWallet), z. B. mit dem Crate `oo7`
   oder `secret-service`. Dann kümmert sich das System um Verschlüsselung und Entsperren.
   Alternativ: eigene Datei, verschlüsselt mit einem Master-Passwort (Argon2 + AES-GCM /
   XChaCha20-Poly1305) – mehr Arbeit, aber unabhängig vom Desktop.
3. **Fragen, ob gespeichert werden soll**: Wenn eine Nachricht ankommt, eine kleine Leiste
   oder ein Popover anzeigen („Passwort für example.com speichern?“).
4. **Ausfüllen**: Beim Laden einer Seite gespeicherte Daten für die Domain suchen und per
   `WebView::evaluate_javascript` (oder einem zweiten User-Script) in die Felder schreiben.
   Achtung: nur auf HTTPS-Seiten und nur für exakt dieselbe Domain ausfüllen!
5. **Übersicht**: Der Button kann ein Popover mit allen Einträgen der aktuellen Seite zeigen
   (kopieren, löschen, Passwort anzeigen).

Zum Testen: `cargo run --release`, dann z. B. <https://the-internet.herokuapp.com/login>.
