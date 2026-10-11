use crate::app::App;
use crate::config::{self, Config, EngineKind, Theme, SEARCH_ENGINES, WALLPAPER_PRESETS};
use crate::memory;
use crate::storage::{Entry, Shelf};

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
  :root { --bg:#0b0e14; --fg:#e8ecf2; --muted:#8f98a8; --card:#121722; --line:#1f2633; --accent:#13b5b1; }
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

pub const WALLPAPER_MAX_BYTES: u64 = 25 * 1024 * 1024;

pub fn wallpaper_path() -> std::path::PathBuf {
    config::data_dir().join("wallpaper")
}

const WALLPAPER_STYLE: &str = r#"
body.wall { color: #f4f6fa; background: #05070d; min-height: 100vh; }
.wall-bg { position: fixed; inset: -40px; z-index: -2; background-size: cover; background-position: center; }
.wall-dim { position: fixed; inset: 0; z-index: -1; }
body.wall h1 .logo { color: #ffffff; text-shadow: 0 2px 18px rgba(0,0,0,.45); }
body.wall h2, body.wall .muted { color: rgba(244,246,250,.82); text-shadow: 0 1px 8px rgba(0,0,0,.5); }
body.wall a { color: #7ff0e8; }
body.wall form.search input { background: rgba(10,14,22,.55); color: #fff; border-color: rgba(255,255,255,.22);
  -webkit-backdrop-filter: blur(14px); backdrop-filter: blur(14px); }
body.wall form.search input::placeholder { color: rgba(255,255,255,.65); }
body.wall .tile { background: rgba(10,14,22,.5); border-color: rgba(255,255,255,.14); color: #fff;
  -webkit-backdrop-filter: blur(14px); backdrop-filter: blur(14px); }
body.wall .tile:hover { border-color: #13b5b1; background: rgba(10,14,22,.65); }
.corner { position: fixed; right: 18px; bottom: 16px; font-size: 13px; padding: 7px 14px; border-radius: 999px;
  background: rgba(10,14,22,.5); color: #fff !important; border: 1px solid rgba(255,255,255,.18);
  -webkit-backdrop-filter: blur(10px); backdrop-filter: blur(10px); }
body:not(.wall) .corner { background: var(--card); color: var(--fg) !important; border-color: var(--line); }
.presets { display:flex; gap:28px 10px; flex-wrap: wrap; padding-bottom: 20px; }
.preset { width: 92px; height: 58px; border-radius: 12px; border: 2px solid var(--line); display:block; position: relative; }
.preset.selected { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 30%, transparent); }
.preset span { position:absolute; left:0; right:0; bottom:-20px; font-size: 11px; text-align:center; color: var(--muted); }
.progress { height: 10px; border-radius: 999px; background: var(--line); overflow: hidden; margin: 18px 0 8px; }
.progress div { height: 100%; background: linear-gradient(90deg, #7d4698, #13b5b1); }
.badge-ok { color: #3fbf6f; font-weight: 600; }
.badge-err { color: var(--danger); font-weight: 600; }
"#;

pub fn layout(title: &str, class: &str, body: &str) -> String {
    layout_with(title, class, "", body)
}

fn layout_with(title: &str, class: &str, body_class: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data: poezi:; form-action poezi: https: http:; base-uri 'none'\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\"><title>{}</title><style>{STYLE}{WALLPAPER_STYLE}</style></head>\
         <body class=\"{body_class}\"><main class=\"{class}\">{body}</main></body></html>",
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
        "wallpaper" => return wallpaper_image(),
        "tor" => tor_page(app),
        "history" => history_page(app, &params, authorized),
        "bookmarks" => bookmarks_page(app, &params, authorized),
        "settings" => settings_page(app, &params, authorized),
        "about" => about_page(app),
        _ => error_page("Nicht gefunden", &format!("Die interne Seite <code>{}</code> gibt es nicht.", escape(uri))),
    };
    ("text/html".into(), html.into_bytes())
}

fn wallpaper_image() -> (String, Vec<u8>) {
    let path = wallpaper_path();
    let too_big = std::fs::metadata(&path).map_or(true, |m| m.len() > WALLPAPER_MAX_BYTES);
    let bytes = if too_big { Vec::new() } else { std::fs::read(&path).unwrap_or_default() };
    let (mime, _) = gtk::gio::content_type_guess(None::<&str>, &bytes);
    let mime = gtk::gio::content_type_get_mime_type(&mime).map(|m| m.to_string()).unwrap_or_default();
    if !mime.starts_with("image/") || mime.contains("svg") {
        return ("image/png".into(), Vec::new());
    }
    (mime, bytes)
}

fn wallpaper_css(config: &Config) -> Option<String> {
    let background = match config.wallpaper.as_str() {
        "" => return None,
        "custom" => {
            if !wallpaper_path().is_file() {
                return None;
            }
            "url(poezi://wallpaper)".to_string()
        }
        id => WALLPAPER_PRESETS.iter().find(|(p, _, _)| *p == id)?.2.to_string(),
    };
    let dim = config.wallpaper_dim.min(90) as f64 / 100.0;
    let blur = config.wallpaper_blur.min(40);
    Some(format!(
        "<div class=\"wall-bg\" style=\"background: {background}; background-size: cover; background-position: center; filter: blur({blur}px)\"></div>\
         <div class=\"wall-dim\" style=\"background: rgba(0,0,0,{dim})\"></div>"
    ))
}

fn start_page(app: &App) -> String {
    let config = app.config.borrow();
    let tiles_html = favorite_tiles(app, Shelf::Normal);
    let wall = wallpaper_css(&config);
    layout_with(
        "Neuer Tab",
        "start",
        if wall.is_some() { "wall" } else { "" },
        &format!(
            "{}<h1><span class=\"logo\">Poezi<span>Browse</span></span></h1>{}{tiles_html}\
             <a class=\"corner\" href=\"poezi://settings#hintergrund\">Hintergrund ändern</a>",
            wall.unwrap_or_default(),
            search_form(&config)
        ),
    )
}

fn favorite_tiles(app: &App, shelf: Shelf) -> String {
    let tiles: Vec<Entry> = app.storage.bookmarks(shelf).into_iter().rev().take(24).collect();
    let tiles_html = if tiles.is_empty() {
        "<h2>Favoriten</h2><p class=\"muted\">Noch keine Favoriten. Klicke auf den ☆ in der Adressleiste oder drücke Strg+D.</p>".to_string()
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
        format!("<h2>Favoriten</h2><div class=\"tiles\">{items}</div>")
    };
    tiles_html
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
            app.storage.remove_bookmark(Shelf::Normal, url);
        }
    }
    let entries = app.storage.bookmarks(Shelf::Normal);
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

fn tor_page(app: &App) -> String {
    let status = app.tor.status();
    let refresh = if status.running && !status.ready() { "<meta http-equiv=\"refresh\" content=\"1\">" } else { "" };
    let body = if let Some(error) = &status.error {
        format!(
            "<p class=\"badge-err\">{}</p><p>Prüfe die Tor-Einstellungen unter <a href=\"poezi://settings#tor\">Einstellungen</a> \
             und öffne dann einen neuen Tor-Tab.</p>",
            escape(error)
        )
    } else if status.ready() {
        "<p class=\"badge-ok\">Verbunden mit dem Tor-Netzwerk.</p>\
         <p>Alles in Tor-Tabs läuft über Tor. Nichts wird gespeichert, und beim Schließen des letzten Tor-Tabs wird alles vergessen.</p>\
         <form class=\"search\" action=\"https://duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion/\" method=\"get\"><input type=\"search\" name=\"q\" placeholder=\"Über Tor mit DuckDuckGo (.onion) suchen\" autofocus></form>\
         <h2>Prüfen</h2><p><a href=\"https://check.torproject.org/\">check.torproject.org</a> · \
         <a href=\"https://duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion/\">DuckDuckGo als .onion</a></p>\
         <h2>Gut zu wissen</h2><p class=\"muted\">Tor versteckt deine IP-Adresse. Gegen Browser-Fingerprinting schützt der Tor Browser \
         besser als PoeziBrowse. Wenn es um deine Sicherheit geht (Journalismus, Aktivismus), nimm den Tor Browser.</p>"
            .to_string()
            + &favorite_tiles(app, Shelf::Tor)
    } else if status.running {
        format!(
            "<p>Verbinde mit dem Tor-Netzwerk …</p><div class=\"progress\"><div style=\"width:{}%\"></div></div><p class=\"muted\">{} % · {}</p>",
            status.progress,
            status.progress,
            escape(&status.summary)
        )
    } else {
        "<p>Tor ist aus. Öffne einen neuen Tor-Tab, um dich zu verbinden.</p>".to_string()
    };
    layout("Tor", "", &format!("<h1><span class=\"logo\">Tor-<span>Modus</span></span></h1>{body}"))
        .replacen("<head>", &format!("<head>{refresh}"), 1)
}

fn settings_page(app: &App, params: &[(String, String)], authorized: bool) -> String {
    let mut notice = String::new();
    if authorized {
        if let Some(choice) = param(params, "wallpaper") {
            let valid = choice.is_empty() || choice == "custom" || WALLPAPER_PRESETS.iter().any(|(id, _, _)| *id == choice);
            if choice == "pick" {
                if let Some(app) = app.rc() {
                    gtk::glib::idle_add_local_once(move || app.pick_wallpaper());
                }
            } else if valid {
                let mut c = app.config.borrow_mut();
                c.wallpaper = choice.to_string();
                c.save();
            }
        }
    }
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
        c.theme = match text("theme").as_deref() {
            Some("light") => Theme::Light,
            Some("dark") => Theme::Dark,
            _ => Theme::System,
        };
        c.titlebar_pattern = flag("titlebar_pattern");
        c.titlebar_animation = flag("titlebar_animation");
        c.titlebar_fps = number("titlebar_fps", 1, 60, c.titlebar_fps);
        c.wallpaper_dim = number("wallpaper_dim", 0, 90, c.wallpaper_dim);
        c.wallpaper_blur = number("wallpaper_blur", 0, 40, c.wallpaper_blur);
        c.show_favorites = flag("show_favorites");
        c.show_sidebar = flag("show_sidebar");
        c.tor_javascript = flag("tor_javascript");
        c.tor_use_bridges = flag("tor_use_bridges");
        c.tor_bridges = crate::tor::clean_bridges(&text("tor_bridges").unwrap_or_default().lines().map(String::from).collect::<Vec<_>>());
        c.block_allowlist = text("block_allowlist")
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().trim_start_matches("www.").to_ascii_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        c.save();
        *app.config.borrow_mut() = c;
        app.apply_appearance();
        app.layout_changed();
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
    let theme_option = |value: &str, label: &str, theme: Theme| {
        format!("<option value=\"{value}\"{}>{label}</option>", if c.theme == theme { " selected" } else { "" })
    };
    let token = &app.token;
    let preset_link = |id: &str, name: &str, background: &str| {
        format!(
            "<a class=\"preset{}\" href=\"poezi://settings?token={token}&amp;wallpaper={id}#hintergrund\" title=\"{name}\" style=\"background:{}\"><span>{name}</span></a>",
            if c.wallpaper == id { " selected" } else { "" },
            escape(background)
        )
    };
    let mut presets = preset_link("", "Keins", "var(--card)");
    for (id, name, css) in WALLPAPER_PRESETS {
        presets.push_str(&preset_link(id, name, css));
    }
    if wallpaper_path().is_file() {
        presets.push_str(&preset_link("custom", "Eigenes Bild", "url(poezi://wallpaper) center/cover"));
    }
    let wallpaper_rows = [
        row(
            "Hintergrund der Startseite",
            "Klicke auf eine Vorlage oder wähle ein eigenes Bild",
            format!(
                "<div class=\"presets\">{presets}</div><p style=\"margin-top:30px\"><a class=\"button\" href=\"poezi://settings?token={token}&amp;wallpaper=pick#hintergrund\">Eigenes Bild wählen …</a></p>"
            ),
        ),
        row("Abdunkeln (%)", "Damit Suchfeld und Kacheln gut lesbar bleiben", format!("<input type=\"number\" min=\"0\" max=\"90\" name=\"wallpaper_dim\" value=\"{}\">", c.wallpaper_dim)),
        row("Unschärfe (Pixel)", "0 = gestochen scharf", format!("<input type=\"number\" min=\"0\" max=\"40\" name=\"wallpaper_blur\" value=\"{}\">", c.wallpaper_blur)),
    ];
    let transports: String = crate::tor::TRANSPORTS
        .iter()
        .map(|t| {
            let found = crate::tor::find_binary(t.binaries).is_some();
            format!(
                "{} <span class=\"{}\">{}</span>",
                t.name,
                if found { "badge-ok" } else { "muted" },
                if found { "✓ installiert".to_string() } else { format!("fehlt (Paket {})", t.package) }
            )
        })
        .collect::<Vec<_>>()
        .join("<br>");
    let tor_installed = crate::tor::find_binary(&["tor"]).is_some();
    let tor_rows = [
        row(
            "Tor",
            "Tor-Tabs öffnest du über die Seitenleiste, das Menü oder Strg+Umschalt+U",
            if tor_installed { "<span class=\"badge-ok\">✓ installiert</span>".into() } else { "<span class=\"badge-err\">nicht installiert – sudo pacman -S tor</span>".into() },
        ),
        row("JavaScript im Tor-Modus", "Aus ist sicherer, viele Seiten funktionieren dann aber nicht", format!("<input type=\"checkbox\" name=\"tor_javascript\"{}>", checked(c.tor_javascript))),
        row("Brücken verwenden", "Wenn Tor in deinem Netz blockiert wird", format!("<input type=\"checkbox\" name=\"tor_use_bridges\"{}>", checked(c.tor_use_bridges))),
        row(
            "Brücken",
            "Eine Brücke pro Zeile. Brücken bekommst du auf <a href=\"https://bridges.torproject.org/options\">bridges.torproject.org</a>",
            format!("<textarea name=\"tor_bridges\" placeholder=\"obfs4 1.2.3.4:443 FINGERPRINT cert=… iat-mode=0\">{}</textarea>", escape(&c.tor_bridges.join("\n"))),
        ),
        row("Brückenarten", "Programme für die Brücken (Pluggable Transports)", transports),
    ];
    let appearance = [
        row(
            "Erscheinungsbild",
            "Für Menüs, Dialoge und Seiten, die hell und dunkel können",
            format!(
                "<select name=\"theme\">{}{}{}</select>",
                theme_option("system", "Wie das System", Theme::System),
                theme_option("light", "Hell", Theme::Light),
                theme_option("dark", "Dunkel", Theme::Dark)
            ),
        ),
        row(
            "Muster in der Fensterleiste",
            "Die Höhenlinien hinter Adressleiste und Fensterknöpfen. Ohne sie bleibt der Farbverlauf.",
            format!("<input type=\"checkbox\" name=\"titlebar_pattern\"{}>", checked(c.titlebar_pattern)),
        ),
        row(
            "Muster bewegen",
            "Die Linien wandern langsam durch die Leiste. Kostet ein wenig Rechenzeit.",
            format!("<input type=\"checkbox\" name=\"titlebar_animation\"{}>", checked(c.titlebar_animation)),
        ),
        row(
            "Bilder je Sekunde",
            "Wie oft die Leiste neu gezeichnet wird. Die Geschwindigkeit bleibt gleich – mehr heißt glatter und teurer.",
            format!("<input type=\"number\" min=\"1\" max=\"60\" name=\"titlebar_fps\" value=\"{}\">", c.titlebar_fps),
        ),
        row("Favoritenleiste", "Deine Lesezeichen unter der Adressleiste", format!("<input type=\"checkbox\" name=\"show_favorites\"{}>", checked(c.show_favorites))),
        row("Seitenleiste links", "Lesezeichen, Verlauf, Downloads, Tor und mehr", format!("<input type=\"checkbox\" name=\"show_sidebar\"{}>", checked(c.show_sidebar))),
    ];
    layout(
        "Einstellungen",
        "",
        &format!(
            "<h1>Einstellungen</h1>{notice}<form action=\"poezi://settings\" method=\"get\">\
             <input type=\"hidden\" name=\"token\" value=\"{}\"><input type=\"hidden\" name=\"save\" value=\"1\">\
             <h2>Erscheinungsbild</h2><div class=\"settings\">{}</div>\
             <h2 id=\"hintergrund\">Hintergrund</h2><div class=\"settings\">{}</div>\
             <h2 id=\"tor\">Tor-Modus</h2><div class=\"settings\">{}</div>\
             <h2>Browser</h2><div class=\"settings\">{}</div>\
             <p><button type=\"submit\">Speichern</button></p></form>",
            app.token,
            appearance.concat(),
            wallpaper_rows.concat(),
            tor_rows.concat(),
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
