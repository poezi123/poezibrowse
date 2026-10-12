use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.poezi123.PoeziBrowse";
pub const APP_NAME: &str = "PoeziBrowse";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    #[default]
    Webkit,
    Servo,
}

impl EngineKind {
    pub fn label(self) -> &'static str {
        match self {
            EngineKind::Webkit => "WebKit",
            EngineKind::Servo => "Servo",
        }
    }

    pub fn available(self) -> bool {
        match self {
            EngineKind::Webkit => true,
            EngineKind::Servo => false,
        }
    }
}

pub const SEARCH_ENGINES: &[(&str, &str, &str)] = &[
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q={}"),
    ("startpage", "Startpage", "https://www.startpage.com/do/search?q={}"),
    ("brave", "Brave Search", "https://search.brave.com/search?q={}"),
    ("ecosia", "Ecosia", "https://www.ecosia.org/search?q={}"),
    ("qwant", "Qwant", "https://www.qwant.com/?q={}"),
    ("google", "Google", "https://www.google.com/search?q={}"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub homepage: String,
    pub search_engine: String,
    pub engine: EngineKind,
    pub tab_sleep_minutes: u32,
    pub tabs_per_process: u32,
    pub memory_limit_mb: u32,
    pub ram_budget_mb: u32,
    pub saver_mode: bool,
    pub block_ads: bool,
    pub block_allowlist: Vec<String>,
    pub restore_session: bool,
    pub autoplay_sound: bool,
    pub default_zoom: f64,
    pub download_dir: String,
    pub hardware_acceleration: bool,
    pub theme: Theme,
    pub titlebar_pattern: bool,
    pub titlebar_animation: bool,
    pub titlebar_fps: u32,
    pub wallpaper: String,
    pub wallpaper_dim: u32,
    pub wallpaper_blur: u32,
    pub show_favorites: bool,
    pub show_sidebar: bool,
    pub tor_javascript: bool,
    pub tor_use_bridges: bool,
    pub tor_bridges: Vec<String>,
    pub password_manager: String,
    pub pm_database: String,
    pub pm_keyfile: String,
    pub pm_remember: Remember,
    pub ignore_media_warning: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Remember {
    #[default]
    Week,
    Session,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Private,
    Tor,
}

impl Mode {
    pub fn is_private(self) -> bool {
        self != Mode::Normal
    }
}

pub const WALLPAPER_PRESETS: &[(&str, &str, &str)] = &[
    ("aurora", "Aurora", "radial-gradient(circle at 20% 20%, #0e9c92 0%, transparent 45%), radial-gradient(circle at 80% 30%, #7c42de 0%, transparent 45%), radial-gradient(circle at 50% 90%, #1d3fbf 0%, transparent 55%), #070a14"),
    ("nacht", "Nachtblau", "linear-gradient(160deg, #02050e 0%, #0b1638 55%, #1a0730 100%)"),
    ("sonnenuntergang", "Sonnenuntergang", "linear-gradient(170deg, #1b1035 0%, #6a1f5c 45%, #d9485f 75%, #f2a65a 100%)"),
    ("wald", "Wald", "linear-gradient(160deg, #04140f 0%, #0c3b2e 50%, #1f6f50 100%)"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            homepage: "poezi://start".into(),
            search_engine: "duckduckgo".into(),
            engine: EngineKind::Webkit,
            tab_sleep_minutes: 10,
            tabs_per_process: 6,
            memory_limit_mb: 1024,
            ram_budget_mb: 1536,
            saver_mode: true,
            block_ads: true,
            block_allowlist: Vec::new(),
            restore_session: true,
            autoplay_sound: false,
            default_zoom: 1.0,
            download_dir: String::new(),
            hardware_acceleration: true,
            theme: Theme::System,
            titlebar_pattern: true,
            titlebar_animation: true,
            titlebar_fps: 16,
            wallpaper: String::new(),
            wallpaper_dim: 35,
            wallpaper_blur: 0,
            show_favorites: true,
            show_sidebar: true,
            tor_javascript: true,
            tor_use_bridges: false,
            tor_bridges: Vec::new(),
            password_manager: String::new(),
            pm_database: String::new(),
            pm_keyfile: String::new(),
            pm_remember: Remember::Week,
            ignore_media_warning: false,
        }
    }
}

impl Config {
    pub fn load() -> Config {
        let path = config_dir().join("config.toml");
        let mut config: Config = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| match toml::from_str(&text) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("poezibrowse: ignoring broken {}: {e}", path.display());
                    None
                }
            })
            .unwrap_or_default();
        if !config.engine.available() {
            config.engine = EngineKind::Webkit;
        }
        config.tabs_per_process = config.tabs_per_process.max(1);
        config.titlebar_fps = config.titlebar_fps.clamp(1, 60);
        config
    }

    pub fn save(&self) {
        let dir = config_dir();
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = write_private(&dir.join("config.toml"), text.as_bytes()) {
                    eprintln!("poezibrowse: could not save settings: {e}");
                }
            }
            Err(e) => eprintln!("poezibrowse: could not serialize settings: {e}"),
        }
    }

    pub fn search_url(&self, query: &str) -> String {
        let template = SEARCH_ENGINES
            .iter()
            .find(|(id, _, _)| *id == self.search_engine)
            .unwrap_or(&SEARCH_ENGINES[0])
            .2;
        template.replace("{}", &url_encode(query))
    }

    pub fn download_dir(&self) -> PathBuf {
        if !self.download_dir.is_empty() {
            return PathBuf::from(&self.download_dir);
        }
        dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
    }

    pub fn is_allowlisted(&self, host: &str) -> bool {
        self.block_allowlist
            .iter()
            .any(|h| host == h || host.ends_with(&format!(".{h}")))
    }
}

pub fn private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

pub fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(dir) = path.parent() {
        private_dir(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(data)
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_default().join("poezibrowse")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("poezibrowse")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_default().join("poezibrowse")
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub const TOR_SEARCH: &str = "https://duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion/?q={}";

pub fn resolve_input_for(input: &str, config: &Config, mode: Mode) -> String {
    let resolved = resolve_input(input, config);
    if mode == Mode::Tor && resolved == config.search_url(input.trim()) && !input.trim().is_empty() {
        return TOR_SEARCH.replace("{}", &url_encode(input.trim()));
    }
    resolved
}

pub fn resolve_input(input: &str, config: &Config) -> String {
    let input = input.trim();
    if input.is_empty() {
        return config.homepage.clone();
    }
    let lower = input.to_ascii_lowercase();
    let has_scheme = ["http://", "https://", "file://", "poezi://", "about:", "data:", "view-source:"]
        .iter()
        .any(|s| lower.starts_with(s));
    if has_scheme {
        return input.to_string();
    }
    if input.starts_with('/') {
        return format!("file://{input}");
    }
    let looks_like_host = !input.contains(char::is_whitespace)
        && (input.contains('.') || lower.starts_with("localhost") || input.contains(':'))
        && !input.starts_with('.')
        && !input.ends_with('.');
    if looks_like_host {
        let host = input.split(['/', '?', '#']).next().unwrap_or("");
        let is_local = host.starts_with("localhost") || host.split(':').next().is_some_and(|h| h.parse::<std::net::Ipv4Addr>().is_ok());
        let scheme = if is_local { "http" } else { "https" };
        return format!("{scheme}://{input}");
    }
    config.search_url(input)
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = if host.starts_with('[') {
        host.split(']').next()?.trim_start_matches('[')
    } else {
        host.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_input() {
        let c = Config::default();
        assert_eq!(resolve_input("example.com", &c), "https://example.com");
        assert_eq!(resolve_input("localhost:8080/x", &c), "http://localhost:8080/x");
        assert_eq!(resolve_input("192.168.0.1", &c), "http://192.168.0.1");
        assert_eq!(resolve_input("https://a.b/c", &c), "https://a.b/c");
        assert_eq!(resolve_input("rust lang", &c), "https://duckduckgo.com/?q=rust+lang");
        assert_eq!(resolve_input("rust", &c), "https://duckduckgo.com/?q=rust");
        assert_eq!(resolve_input("/etc/hosts", &c), "file:///etc/hosts");
        assert!(resolve_input_for("rust lang", &c, Mode::Tor).contains(".onion/?q=rust+lang"));
        assert_eq!(resolve_input_for("example.com", &c, Mode::Tor), "https://example.com");
    }

    #[test]
    fn hosts() {
        assert_eq!(host_of("https://www.Example.com:443/x?y").as_deref(), Some("www.example.com"));
        assert_eq!(host_of("poezi://start").as_deref(), Some("start"));
        assert_eq!(host_of("about:blank"), None);
    }

    #[test]
    fn url_coding_roundtrip() {
        let s = "a b&c=ä/ü%";
        assert_eq!(url_decode(&url_encode(s)), s);
        assert_eq!(url_decode("100%"), "100%");
    }

    #[test]
    fn allowlist() {
        let c = Config { block_allowlist: vec!["example.com".into()], ..Config::default() };
        assert!(c.is_allowlisted("example.com"));
        assert!(c.is_allowlisted("www.example.com"));
        assert!(!c.is_allowlisted("badexample.com"));
    }
}
