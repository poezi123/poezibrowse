use crate::app::App;
use crate::config::{self, Config, EngineKind, SEARCH_ENGINES};
use crate::memory;
use crate::storage::Entry;

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = r#"
:root { --bg:#f6f7f9; --fg:#1d2128; --muted:#69707d; --card:#fff; --line:#e2e5ea; --accent:#0E9C92; --accent2:#7C42DE; --danger:#c43c3c; }
@media (prefers-color-scheme: dark) {
  :root { --bg:#16181d; --fg:#e7e9ee; --muted:#959cab; --card:#1f2229; --line:#2e323b; }
}
* { box-sizing: border-box; }
body { margin:0; font: 15px/1.5 system-ui, sans-serif; background:var(--bg); color:var(--fg); }
main { max-width: 820px; margin: 0 auto; padding: 48px 20px 60px; }
main.start { padding-top: 14vh; }
h1 { font-weight: 650; letter-spacing: -0.02em; margin: 0 0 24px; font-size: 28px; }
h1 .logo { font-size: 40px; }
h1 .logo { color: var(--fg); }
h1 .logo span { color: var(--accent); }
h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); margin: 36px 0 10px; font-weight: 600; }
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
input, select, textarea, button { font: inherit; color: var(--fg); }
input[type=text], input[type=search], input[type=number], select, textarea {
  padding: 8px 12px; border-radius: 10px; border: 1px solid var(--line); background: var(--card); outline: none; }
input:focus, select:focus, textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
button, .button { padding: 8px 16px; border-radius: 10px; border: 0; background: var(--accent); color: #fff; cursor: pointer; font-weight: 600; }
.button.danger { background: var(--danger); }
form.search { display:flex; gap:8px; }
form.search input { flex:1; font-size:17px; padding:13px 18px; border-radius:14px; }
.muted { color: var(--muted); font-size: 13px; }
.tiles { display:grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 12px; }
.tile { display:flex; flex-direction:column; align-items:center; gap:8px; padding:16px 10px; border-radius:14px;
  background: var(--card); border: 1px solid var(--line); color: var(--fg); text-align:center; min-width:0; }
.tile:hover { border-color: var(--accent); text-decoration:none; }
.avatar { width:40px; height:40px; border-radius:12px; display:grid; place-items:center; color:#fff; font-weight:700; font-size:18px; flex:none; }
.avatar.small { width:28px; height:28px; border-radius:8px; font-size:13px; }
.tile .name { font-size: 13px; width:100%; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.list { background: var(--card); border:1px solid var(--line); border-radius:14px; overflow:hidden; }
.row { display:flex; align-items:center; gap:12px; padding:9px 14px; border-top:1px solid var(--line); min-width:0; }
.row:first-child { border-top:0; }
.row .time { color: var(--muted); font-size:12px; width:44px; flex:none; font-variant-numeric: tabular-nums; }
.row .text { flex:1; min-width:0; }
.row .text a, .row .text .url { display:block; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.row .url { color: var(--muted); font-size:12px; }
.row .x { color: var(--muted); padding: 2px 8px; border-radius: 8px; flex:none; }
.row .x:hover { background: var(--bg); color: var(--danger); text-decoration:none; }
.toolbar { display:flex; gap:8px; align-items:center; margin-bottom: 16px; flex-wrap: wrap; }
.toolbar input { flex:1; min-width: 200px; }
.empty { padding: 28px; text-align:center; color: var(--muted); }
.settings { display:grid; grid-template-columns: minmax(0,1fr) minmax(0,1.2fr); gap: 14px 20px; align-items:center;
  background: var(--card); border:1px solid var(--line); border-radius:14px; padding: 18px 20px; }
.settings label.title { font-weight: 600; }
.settings .hint { display:block; color: var(--muted); font-size: 12px; font-weight: 400; }
.settings input[type=text], .settings select, .settings textarea, .settings input[type=number] { width: 100%; }
.settings textarea { min-height: 80px; resize: vertical; }
.notice { background: color-mix(in srgb, var(--accent) 14%, var(--card)); border:1px solid var(--accent); border-radius: 12px; padding: 10px 14px; margin-bottom: 18px; }
table.stats { border-collapse: collapse; width: 100%; }
table.stats td { padding: 7px 0; border-top: 1px solid var(--line); }
table.stats td:last-child { text-align: right; font-variant-numeric: tabular-nums; }
@media (max-width: 600px) { .settings { grid-template-columns: 1fr; } }
"#;

pub fn layout(title: &str, class: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\"><title>{}</title><style>{STYLE}</style></head>\
         <body><main class=\"{class}\">{body}</main></body></html>",
        escape(title)
    )
}

pub fn error_page(title: &str, message_html: &str) -> String {
    layout(title, "", &format!("<h1>{}</h1><p>{message_html}</p>", escape(title)))
}

fn query(uri: &str) -> Vec<(String, String)> {
    let Some((_, q)) = uri.split_once('?') else { return Vec::new() };
    let q = q.split('#').next().unwrap_or("");
    q.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (config::url_decode(k), config::url_decode(v))
        })
        .collect()
}

fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn display_title(e: &Entry) -> String {
    if e.title.trim().is_empty() { e.url.clone() } else { e.title.clone() }
}

fn short_host(url: &str) -> String {
    config::host_of(url).map(|h| h.trim_start_matches("www.").to_string()).unwrap_or_else(|| url.to_string())
}

fn avatar(url: &str) -> String {
    const COLORS: [&str; 8] = ["#0E9C92", "#7C42DE", "#D9485F", "#E08A1E", "#2F7DE1", "#3A9D4A", "#B04BB3", "#5B6B7F"];
    let host = short_host(url);
    let labels: Vec<&str> = host.split('.').collect();
    let name = if labels.len() >= 2 { labels[labels.len() - 2] } else { host.as_str() };
    let hash = name.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let letter = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    format!("<span class=\"avatar\" style=\"background:{}\">{}</span>", COLORS[hash as usize % COLORS.len()], escape(&letter))
}

fn local_time(unix: i64) -> (String, String) {
    let dt = gtk::glib::DateTime::from_unix_local(unix).ok();
    let day = dt.as_ref().and_then(|d| d.format("%A, %e. %B %Y").ok()).map(|s| s.to_string()).unwrap_or_default();
    let time = dt.as_ref().and_then(|d| d.format("%H:%M").ok()).map(|s| s.to_string()).unwrap_or_default();
    (day, time)
}

fn search_form_target(config: &Config) -> (String, String) {
    let url = config.search_url("");
    let (base, query) = url.split_once('?').unwrap_or((&url, "q="));
    let param = query
        .split('&')
        .find(|kv| kv.ends_with('='))
        .map(|kv| kv.trim_end_matches('='))
        .unwrap_or("q");
    (base.to_string(), param.to_string())
}

fn search_form(config: &Config) -> String {
    let (action, param) = search_form_target(config);
    format!(
        "<form class=\"search\" action=\"{}\" method=\"get\">\
           <input type=\"search\" name=\"{}\" placeholder=\"Mit {} suchen\" autofocus autocomplete=\"off\">\
         </form>",
        escape(&action),
        escape(&param),
        escape(SEARCH_ENGINES.iter().find(|(id, _, _)| *id == config.search_engine).map(|e| e.1).unwrap_or("DuckDuckGo"))
    )
}

pub fn handle(uri: &str, app: &App) -> (String, Vec<u8>) {
    let rest = uri.trim_start_matches("poezi://");
    let page = rest.split(['?', '#', '/']).next().unwrap_or("");
    let params = query(uri);
    let authorized = param(&params, "token") == Some(app.token.as_str());
    let html = match page {
        "start" | "" => start_page(app),
        "history" => history_page(app, &params, authorized),
        "bookmarks" => bookmarks_page(app, &params, authorized),
        "settings" => settings_page(app, &params, authorized),
        "about" => about_page(app),
        _ => error_page("Nicht gefunden", &format!("Die interne Seite <code>{}</code> gibt es nicht.", escape(uri))),
    };
    ("text/html".into(), html.into_bytes())
}

fn start_page(app: &App) -> String {
    let config = app.config.borrow();
    let mut tiles: Vec<Entry> = app.storage.bookmarks().into_iter().take(12).collect();
    let mut heading = "Lesezeichen";
    if tiles.is_empty() {
        tiles = app.storage.most_visited(8);
        heading = "Oft besucht";
    }
    let tiles_html = if tiles.is_empty() {
        String::new()
    } else {
        let items: String = tiles
            .iter()
            .map(|e| {
                format!(
                    "<a class=\"tile\" href=\"{}\" title=\"{}\">{}<span class=\"name\">{}</span></a>",
                    escape(&e.url),
                    escape(&e.url),
                    avatar(&e.url),
                    escape(&display_title(e))
                )
            })
            .collect();
        format!("<h2>{heading}</h2><div class=\"tiles\">{items}</div>")
    };
    layout(
        "Neuer Tab",
        "start",
        &format!(
            "<h1><span class=\"logo\">Poezi<span>Browse</span></span></h1>{}{tiles_html}\
             <p class=\"muted\" style=\"margin-top:40px\"><a href=\"poezi://bookmarks\">Lesezeichen</a> · \
             <a href=\"poezi://history\">Verlauf</a> · <a href=\"poezi://settings\">Einstellungen</a> · \
             <a href=\"poezi://about\">Über</a></p>",
            search_form(&config)
        ),
    )
}

fn history_page(app: &App, params: &[(String, String)], authorized: bool) -> String {
    let mut notice = String::new();
    if authorized {
        if param(params, "clear").is_some() {
            app.storage.clear_history();
            notice = "<div class=\"notice\">Verlauf gelöscht.</div>".into();
        }
        if let Some(url) = param(params, "delete") {
            app.storage.delete_history_entry(url);
        }
    }
    let search = param(params, "q").unwrap_or("");
    let entries = app.storage.history(search, 500);
    let token = &app.token;

    let mut body = format!(
        "<h1>Verlauf</h1>{notice}\
         <form class=\"toolbar\" action=\"poezi://history\" method=\"get\">\
           <input type=\"search\" name=\"q\" value=\"{}\" placeholder=\"Verlauf durchsuchen\">\
           <button type=\"submit\">Suchen</button>\
           <a class=\"button danger\" href=\"poezi://history?token={token}&amp;clear=1\">Alles löschen</a>\
         </form>",
        escape(search)
    );
    if entries.is_empty() {
        body.push_str("<div class=\"list\"><div class=\"empty\">Nichts gefunden.</div></div>");
    }
    let mut current_day = String::new();
    for e in &entries {
        let (day, time) = local_time(e.time);
        if day != current_day {
            if !current_day.is_empty() {
                body.push_str("</div>");
            }
            body.push_str(&format!("<h2>{}</h2><div class=\"list\">", escape(&day)));
            current_day = day;
        }
        body.push_str(&format!(
            "<div class=\"row\"><span class=\"time\">{time}</span>\
             <div class=\"text\"><a href=\"{u}\">{t}</a><span class=\"url\">{u}</span></div>\
             <a class=\"x\" title=\"Aus dem Verlauf entfernen\" href=\"poezi://history?token={token}&amp;q={q}&amp;delete={ue}\">✕</a></div>",
            u = escape(&e.url),
            t = escape(&display_title(e)),
            q = escape(&config::url_encode(search)),
            ue = escape(&config::url_encode(&e.url)),
        ));
    }
    if !current_day.is_empty() {
        body.push_str("</div>");
    }
    layout("Verlauf", "", &body)
}

fn bookmarks_page(app: &App, params: &[(String, String)], authorized: bool) -> String {
    if authorized {
        if let Some(url) = param(params, "remove") {
            app.storage.remove_bookmark(url);
        }
    }
    let entries = app.storage.bookmarks();
    let token = &app.token;
    let mut body = String::from("<h1>Lesezeichen</h1><div class=\"list\">");
    if entries.is_empty() {
        body.push_str("<div class=\"empty\">Noch keine Lesezeichen. Klicke auf den Stern in der Adressleiste oder drücke Strg+D.</div>");
    }
    for e in &entries {
        body.push_str(&format!(
            "<div class=\"row\">{}<div class=\"text\"><a href=\"{u}\">{t}</a><span class=\"url\">{u}</span></div>\
             <a class=\"x\" title=\"Lesezeichen entfernen\" href=\"poezi://bookmarks?token={token}&amp;remove={ue}\">✕</a></div>",
            avatar(&e.url).replace("class=\"avatar\"", "class=\"avatar small\""),
            u = escape(&e.url),
            t = escape(&display_title(e)),
            ue = escape(&config::url_encode(&e.url)),
        ));
    }
    body.push_str("</div>");
    layout("Lesezeichen", "", &body)
}

fn settings_page(app: &App, params: &[(String, String)], authorized: bool) -> String {
    let mut notice = String::new();
    if authorized && param(params, "save").is_some() {
        let mut c = app.config.borrow().clone();
        let text = |k: &str| param(params, k).map(|v| v.trim().to_string());
        let flag = |k: &str| param(params, k).is_some();
        let number = |k: &str, min: u32, max: u32, old: u32| text(k).and_then(|v| v.parse::<u32>().ok()).map(|v| v.clamp(min, max)).unwrap_or(old);
        if let Some(v) = text("homepage") {
            c.homepage = if v.is_empty() { Config::default().homepage } else { config::resolve_input(&v, &c) };
        }
        if let Some(v) = text("search_engine").filter(|v| SEARCH_ENGINES.iter().any(|(id, _, _)| id == v)) {
            c.search_engine = v;
        }
        c.engine = match text("engine").as_deref() {
            Some("servo") if EngineKind::Servo.available() => EngineKind::Servo,
            _ => EngineKind::Webkit,
        };
        c.tab_sleep_minutes = number("tab_sleep_minutes", 0, 24 * 60, c.tab_sleep_minutes);
        c.tabs_per_process = number("tabs_per_process", 1, 50, c.tabs_per_process);
        c.memory_limit_mb = number("memory_limit_mb", 0, 64 * 1024, c.memory_limit_mb);
        c.ram_budget_mb = number("ram_budget_mb", 0, 256 * 1024, c.ram_budget_mb);
        c.default_zoom = number("default_zoom", 30, 300, (c.default_zoom * 100.0) as u32) as f64 / 100.0;
        c.saver_mode = flag("saver_mode");
        c.block_ads = flag("block_ads");
        c.restore_session = flag("restore_session");
        c.autoplay_sound = flag("autoplay_sound");
        c.hardware_acceleration = flag("hardware_acceleration");
        c.download_dir = text("download_dir").unwrap_or_default();
        c.block_allowlist = text("block_allowlist")
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().trim_start_matches("www.").to_ascii_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        c.save();
        *app.config.borrow_mut() = c;
        notice = "<div class=\"notice\">Gespeichert. Mit * markierte Einstellungen wirken nach einem Neustart von PoeziBrowse.</div>".into();
    }

    let c = app.config.borrow();
    let checked = |b: bool| if b { " checked" } else { "" };
    let engines: String = SEARCH_ENGINES
        .iter()
        .map(|(id, name, _)| format!("<option value=\"{id}\"{}>{name}</option>", if *id == c.search_engine { " selected" } else { "" }))
        .collect();
    let servo_disabled = if EngineKind::Servo.available() { "" } else { " disabled" };
    let row = |title: &str, hint: &str, input: String| {
        format!("<label class=\"title\">{title}<span class=\"hint\">{hint}</span></label><div>{input}</div>")
    };
    let rows = [
        row("Startseite", "Für neue Tabs und Fenster", format!("<input type=\"text\" name=\"homepage\" value=\"{}\">", escape(&c.homepage))),
        row("Suchmaschine", "", format!("<select name=\"search_engine\">{engines}</select>")),
        row(
            "Engine für neue Tabs",
            "Servo ist experimentell",
            format!(
                "<select name=\"engine\"><option value=\"webkit\"{}>WebKit</option><option value=\"servo\"{}{servo_disabled}>Servo{}</option></select>",
                if c.engine == EngineKind::Webkit { " selected" } else { "" },
                if c.engine == EngineKind::Servo { " selected" } else { "" },
                if servo_disabled.is_empty() { "" } else { " (nicht eingebaut)" }
            ),
        ),
        row("Tabs schlafen legen nach", "Minuten ohne Benutzung, 0 = nie", format!("<input type=\"number\" min=\"0\" name=\"tab_sleep_minutes\" value=\"{}\">", c.tab_sleep_minutes)),
        row("RAM-Budget (MB)", "Wird es überschritten, schläft sofort der am längsten unbenutzte Tab ein. 0 = aus", format!("<input type=\"number\" min=\"0\" name=\"ram_budget_mb\" value=\"{}\">", c.ram_budget_mb)),
        row("Tabs pro Prozess *", "Mehr = weniger RAM, aber ein Absturz trifft mehrere Tabs", format!("<input type=\"number\" min=\"1\" max=\"50\" name=\"tabs_per_process\" value=\"{}\">", c.tabs_per_process)),
        row("Speicherlimit pro Prozess (MB) *", "Darüber gibt WebKit Caches frei, 0 = Standard", format!("<input type=\"number\" min=\"0\" name=\"memory_limit_mb\" value=\"{}\">", c.memory_limit_mb)),
        row("RAM-Sparmodus *", "Kleinere Caches, kein Seiten-Cache für Zurück/Vor", format!("<input type=\"checkbox\" name=\"saver_mode\"{}>", checked(c.saver_mode))),
        row("Werbung und Tracker blockieren *", "EasyList, EasyPrivacy, EasyList Germany", format!("<input type=\"checkbox\" name=\"block_ads\"{}>", checked(c.block_ads))),
        row("Blocker-Ausnahmen", "Eine Domain pro Zeile", format!("<textarea name=\"block_allowlist\">{}</textarea>", escape(&c.block_allowlist.join("\n")))),
        row("Tabs beim Start wiederherstellen", "", format!("<input type=\"checkbox\" name=\"restore_session\"{}>", checked(c.restore_session))),
        row("Videos mit Ton automatisch abspielen *", "", format!("<input type=\"checkbox\" name=\"autoplay_sound\"{}>", checked(c.autoplay_sound))),
        row("Hardwarebeschleunigung *", "", format!("<input type=\"checkbox\" name=\"hardware_acceleration\"{}>", checked(c.hardware_acceleration))),
        row("Standard-Zoom (%)", "", format!("<input type=\"number\" min=\"30\" max=\"300\" name=\"default_zoom\" value=\"{}\">", (c.default_zoom * 100.0).round())),
        row("Download-Ordner", "Leer = Standard-Downloadordner", format!("<input type=\"text\" name=\"download_dir\" value=\"{}\" placeholder=\"{}\">", escape(&c.download_dir), escape(&c.download_dir().to_string_lossy()))),
    ];
    layout(
        "Einstellungen",
        "",
        &format!(
            "<h1>Einstellungen</h1>{notice}<form action=\"poezi://settings\" method=\"get\">\
             <input type=\"hidden\" name=\"token\" value=\"{}\"><input type=\"hidden\" name=\"save\" value=\"1\">\
             <div class=\"settings\">{}</div><p><button type=\"submit\">Speichern</button></p></form>",
            app.token,
            rows.concat()
        ),
    )
}

fn about_page(app: &App) -> String {
    let c = app.config.borrow();
    let processes: String = memory::breakdown()
        .into_iter()
        .map(|(name, bytes)| format!("<tr><td>{}</td><td>{}</td></tr>", escape(&name), memory::format_bytes(bytes)))
        .collect();
    let blocker = match app.blocker_rules() {
        Some(true) => "aktiv",
        Some(false) => "wird vorbereitet …",
        None => "aus",
    };
    layout(
        "Über PoeziBrowse",
        "",
        &format!(
            "<h1><span class=\"logo\">Poezi<span>Browse</span></span> {}</h1>\
             <p>Ein schlanker Browser für Linux, geschrieben in Rust. Ohne Telemetrie, ohne Konto, ohne Hintergrunddienste.</p>\
             <h2>Arbeitsspeicher</h2><table class=\"stats\">{processes}</table>\
             <h2>Status</h2><table class=\"stats\">\
               <tr><td>Engine für neue Tabs</td><td>{}</td></tr>\
               <tr><td>Servo eingebaut</td><td>{}</td></tr>\
               <tr><td>Werbe- und Trackerblocker</td><td>{blocker}</td></tr>\
               <tr><td>Tabs schlafen nach</td><td>{}</td></tr>\
               <tr><td>Tabs pro Prozess</td><td>{}</td></tr>\
               <tr><td>RAM-Budget</td><td>{}</td></tr></table>\
             <p class=\"muted\" style=\"margin-top:28px\">Quellcode: <a href=\"https://github.com/poezi123/poezibrowse\">github.com/poezi123/poezibrowse</a></p>",
            config::VERSION,
            c.engine.label(),
            if EngineKind::Servo.available() { "ja" } else { "nein" },
            if c.tab_sleep_minutes == 0 { "nie".to_string() } else { format!("{} min", c.tab_sleep_minutes) },
            c.tabs_per_process,
            if c.ram_budget_mb == 0 { "aus".to_string() } else { format!("{} MB", c.ram_budget_mb) },
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_target() {
        let c = Config::default();
        assert_eq!(search_form_target(&c), ("https://duckduckgo.com/".into(), "q".into()));
    }

    #[test]
    fn escapes() {
        assert_eq!(escape("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    }

    #[test]
    fn parses_query() {
        let q = query("poezi://history?token=abc&q=a+b%26c&clear");
        assert_eq!(param(&q, "token"), Some("abc"));
        assert_eq!(param(&q, "q"), Some("a b&c"));
        assert_eq!(param(&q, "clear"), Some(""));
        assert_eq!(param(&q, "nope"), None);
    }
}
