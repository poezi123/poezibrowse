use gtk::glib;
use serde::{Deserialize, Serialize};

use crate::config::{self, EngineKind};
use crate::tab::Snapshot;

#[derive(Serialize, Deserialize, Default)]
pub struct Session {
    pub windows: Vec<WindowState>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct WindowState {
    pub tabs: Vec<TabState>,
    pub active: usize,
}

#[derive(Serialize, Deserialize)]
pub struct TabState {
    pub uri: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub engine: EngineKind,
    #[serde(default)]
    pub state: Option<String>,
}

impl TabState {
    pub fn from_snapshot(s: &Snapshot) -> TabState {
        TabState {
            uri: s.uri.clone(),
            title: s.title.clone(),
            engine: s.engine,
            state: s.state.as_ref().map(|b| glib::base64_encode(b).to_string()),
        }
    }

    pub fn into_snapshot(self) -> Snapshot {
        Snapshot {
            uri: self.uri,
            title: self.title,
            engine: self.engine,
            state: self.state.map(|s| glib::base64_decode(&s)),
        }
    }
}

fn path() -> std::path::PathBuf {
    config::data_dir().join("session.json")
}

pub fn load() -> Session {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(session: &Session) {
    let dir = config::data_dir();
    let Ok(json) = serde_json::to_string(session) else { return };
    let tmp = dir.join("session.json.tmp");
    if config::write_private(&tmp, json.as_bytes()).is_ok() {
        let _ = std::fs::rename(&tmp, path());
    }
}
