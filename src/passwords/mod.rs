mod secret;
mod vault;

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::Orientation;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use webkit2gtk::{
    UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserScript, UserScriptInjectionTime, WebView,
    WebViewExt,
};
use zeroize::Zeroizing;

use crate::app::App;
use crate::config::{self, Mode};
use secret::Kind;
use vault::{Location, Login};

const WORLD: &str = "poezibrowse-passwords";
const HANDLER: &str = "poeziPw";
const FORM_SCRIPT: &str = include_str!("form.js");

#[derive(Clone, Default)]
pub struct PageInfo {
    pub host: String,
    pub login: bool,
    pub signup: bool,
    pub known: bool,
}

#[derive(Default)]
struct Index {
    hashes: HashSet<[u8; 32]>,
    state: Option<(u64, u64)>,
    loaded: bool,
}

pub struct PasswordManager {
    app: Weak<App>,
    cache: secret::Cache,
    index: RefCell<Index>,
    pages: RefCell<Vec<(glib::WeakRef<WebView>, PageInfo)>>,
    recent_submit: RefCell<Option<(String, String, Instant)>>,
    script: std::cell::OnceCell<UserScript>,
    me: Weak<PasswordManager>,
}

fn hmac(key: &[u8], host: &str) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(host.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    (0..text.len()).step_by(2).map(|i| text.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
}

fn index_path() -> std::path::PathBuf {
    secret::dir().join("index")
}

fn index_hashes(key: &[u8], logins: &[Login]) -> HashSet<[u8; 32]> {
    logins
        .iter()
        .filter_map(|l| vault::entry_host(&l.url))
        .map(|host| hmac(key, &host))
        .collect()
}

fn write_index(hashes: &HashSet<[u8; 32]>, state: Option<(u64, u64)>) {
    let (m, s) = state.unwrap_or((0, 0));
    let mut text = format!("state {m} {s}\n");
    for h in hashes {
        text.push_str(&hex(h));
        text.push('\n');
    }
    let _ = config::write_private(&index_path(), text.as_bytes());
}

fn read_index() -> Option<(HashSet<[u8; 32]>, (u64, u64))> {
    let text = std::fs::read_to_string(index_path()).ok()?;
    let mut lines = text.lines();
    let mut head = lines.next()?.split_whitespace().skip(1).map(|v| v.parse::<u64>().ok());
    let state = (head.next()??, head.next()??);
    let hashes = lines.filter_map(unhex).filter_map(|v| v.try_into().ok()).collect();
    Some((hashes, state))
}

fn get_or_create_index_key() -> Zeroizing<String> {
    if let Some(key) = secret::load(Kind::IndexKey) {
        return key;
    }
    let mut bytes = [0u8; 32];
    let _ = getrandom::fill(&mut bytes);
    let key = Zeroizing::new(hex(&bytes));
    secret::store(Kind::IndexKey, &key);
    key
}

impl PasswordManager {
    pub fn new(app: Weak<App>) -> Rc<PasswordManager> {
        Rc::new_cyclic(|me| PasswordManager {
            app,
            cache: secret::Cache::new(),
            index: RefCell::new(Index::default()),
            pages: RefCell::new(Vec::new()),
            recent_submit: RefCell::new(None),
            script: std::cell::OnceCell::new(),
            me: me.clone(),
        })
    }

    fn app(&self) -> Option<Rc<App>> {
        self.app.upgrade()
    }

    pub fn enabled(&self) -> bool {
        self.app().is_some_and(|a| matches!(a.config.borrow().password_manager.as_str(), "keepassxc" | "own"))
    }

    pub fn needs_setup(&self) -> bool {
        self.app().is_some_and(|a| a.config.borrow().password_manager.is_empty())
    }

    fn location(&self) -> Option<Location> {
        let app = self.app()?;
        let c = app.config.borrow();
        match c.password_manager.as_str() {
            "keepassxc" if !c.pm_database.is_empty() => Some(Location {
                path: c.pm_database.clone().into(),
                keyfile: (!c.pm_keyfile.is_empty()).then(|| c.pm_keyfile.clone().into()),
            }),
            "own" => Some(Location { path: secret::dir().join("poezibrowse.kdbx"), keyfile: None }),
            _ => None,
        }
    }

    fn label(&self) -> &'static str {
        if self.app().is_some_and(|a| a.config.borrow().password_manager == "own") { "PoeziBrowse-Passwortmanager" } else { "KeePassXC" }
    }

    pub fn setup_page(self: &Rc<Self>, ucm: &UserContentManager, view: &WebView, mode: Mode) {
        if mode == Mode::Tor {
            return;
        }
        let script = self.script.get_or_init(|| {
            UserScript::for_world(FORM_SCRIPT, UserContentInjectedFrames::TopFrame, UserScriptInjectionTime::End, WORLD, &[], &[])
        });
        ucm.add_script(script);
        ucm.register_script_message_handler_in_world(HANDLER, WORLD);
        let me = self.me.clone();
        let weak_view = view.downgrade();
        ucm.connect_script_message_received(Some(HANDLER), move |_, result| {
            let (Some(me), Some(view)) = (me.upgrade(), weak_view.upgrade()) else { return };
            let Some(value) = result.js_value() else { return };
            let text = Zeroizing::new(javascriptcore::ValueExt::to_str(&value).to_string());
            me.on_message(&view, &text);
        });
        let me = self.me.clone();
        view.connect_load_changed(move |view, event| {
            if event == webkit2gtk::LoadEvent::Started {
                if let Some(me) = me.upgrade() {
                    me.set_page(view, PageInfo::default());
                }
            }
        });
    }

    fn set_page(&self, view: &WebView, info: PageInfo) {
        let mut pages = self.pages.borrow_mut();
        pages.retain(|(w, _)| w.upgrade().is_some());
        match pages.iter_mut().find(|(w, _)| w.upgrade().as_ref() == Some(view)) {
            Some((_, existing)) => *existing = info,
            None => pages.push((view.downgrade(), info)),
        }
        drop(pages);
        if let Some(app) = self.app() {
            app.password_state_changed();
        }
    }

    pub fn page_info(&self, view: &WebView) -> PageInfo {
        self.pages
            .borrow()
            .iter()
            .find(|(w, _)| w.upgrade().as_ref() == Some(view))
            .map(|(_, i)| i.clone())
            .unwrap_or_default()
    }

    fn secure_host(view: &WebView) -> Option<String> {
        let uri = view.uri()?.to_string();
        if !uri.starts_with("https://") {
            return None;
        }
        config::host_of(&uri)
    }

    fn on_message(self: &Rc<Self>, view: &WebView, text: &str) {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(text) else { return };
        let Some(host) = Self::secure_host(view) else { return };
        match message.get("type").and_then(|t| t.as_str()) {
            Some("fields") => {
                let login = message.get("login").and_then(|v| v.as_bool()).unwrap_or(false);
                let signup = message.get("signup").and_then(|v| v.as_bool()).unwrap_or(false);
                let me = self.clone();
                let view = view.clone();
                self.ensure_index(move || {
                    let known = login && me.host_known(&host);
                    me.set_page(&view, PageInfo { host, login, signup, known });
                });
            }
            Some("submit") if self.enabled() => {
                let username = message.get("username").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let password = Zeroizing::new(message.get("password").and_then(|v| v.as_str()).unwrap_or_default().to_string());
                if password.is_empty() {
                    return;
                }
                {
                    let mut recent = self.recent_submit.borrow_mut();
                    if let Some((h, u, at)) = recent.as_ref() {
                        if *h == host && *u == username && at.elapsed() < Duration::from_secs(10) {
                            return;
                        }
                    }
                    *recent = Some((host.clone(), username.clone(), Instant::now()));
                }
                self.on_submit(view, host, username, password);
            }
            _ => {}
        }
    }

    fn host_known(&self, host: &str) -> bool {
        let Some(key) = self.cache.get(Kind::IndexKey) else { return false };
        let index = self.index.borrow();
        vault::candidate_hosts(host).iter().any(|h| index.hashes.contains(&hmac(key.as_bytes(), h)))
    }

    fn ensure_index(self: &Rc<Self>, then: impl FnOnce() + 'static) {
        if !self.enabled() {
            then();
            return;
        }
        let loaded = self.index.borrow().loaded && self.cache.get(Kind::IndexKey).is_some();
        let stale = self.location().and_then(|l| vault::file_state(&l.path)) != self.index.borrow().state;
        if loaded && !stale {
            then();
            return;
        }
        let me = self.clone();
        let location = self.location();
        let cached_master = self.cache.get(Kind::Master);
        glib::MainContext::default().spawn_local(async move {
            let result = gio::spawn_blocking(move || {
                let key = get_or_create_index_key();
                let current = location.as_ref().and_then(|l| vault::file_state(&l.path));
                let stored = read_index();
                if let Some((hashes, state)) = stored.as_ref() {
                    if Some(*state) == current {
                        return (key, hashes.clone(), current, None);
                    }
                }
                let master = cached_master.or_else(|| secret::load(Kind::Master));
                if let (Some(location), Some(master)) = (location.as_ref(), master.as_ref()) {
                    if let Ok(logins) = vault::open(location, master) {
                        let hashes = index_hashes(key.as_bytes(), &logins);
                        write_index(&hashes, current);
                        return (key, hashes, current, Some(master.clone()));
                    }
                }
                let hashes = stored.map(|(h, _)| h).unwrap_or_default();
                let state = if hashes.is_empty() { None } else { current };
                (key, hashes, state, None)
            })
            .await;
            if let Ok((key, hashes, state, master)) = result {
                me.cache.put(Kind::IndexKey, &key);
                if let Some(master) = master {
                    me.cache.put(Kind::Master, &master);
                }
                *me.index.borrow_mut() = Index { hashes, state, loaded: true };
            }
            then();
        });
    }

    fn with_master(self: &Rc<Self>, parent: &gtk::Window, then: impl FnOnce(Zeroizing<String>) + 'static) {
        if let Some(master) = self.cache.get(Kind::Master) {
            then(master);
            return;
        }
        let me = self.clone();
        let parent = parent.clone();
        glib::MainContext::default().spawn_local(async move {
            let loaded = gio::spawn_blocking(|| secret::load(Kind::Master)).await.ok().flatten();
            match loaded {
                Some(master) => {
                    me.cache.put(Kind::Master, &master);
                    then(master);
                }
                None => me.unlock_dialog(&parent, Box::new(then)),
            }
        });
    }

    fn with_logins(self: &Rc<Self>, parent: &gtk::Window, then: impl FnOnce(Vec<Login>, Zeroizing<String>) + 'static) {
        let Some(location) = self.location() else { return };
        let me = self.clone();
        let parent2 = parent.clone();
        self.with_master(parent, move |master| {
            let key = me.cache.get(Kind::IndexKey);
            glib::MainContext::default().spawn_local(async move {
                let master2 = master.clone();
                let location2 = location.clone();
                let result = gio::spawn_blocking(move || {
                    let logins = vault::open(&location2, &master2)?;
                    let key = key.unwrap_or_else(get_or_create_index_key);
                    let hashes = index_hashes(key.as_bytes(), &logins);
                    let state = vault::file_state(&location2.path);
                    write_index(&hashes, state);
                    Ok::<_, String>((logins, hashes, state, key))
                })
                .await
                .unwrap_or_else(|_| Err("Fehler".into()));
                match result {
                    Ok((logins, hashes, state, key)) => {
                        me.cache.put(Kind::IndexKey, &key);
                        *me.index.borrow_mut() = Index { hashes, state, loaded: true };
                        then(logins, master);
                    }
                    Err(e) => {
                        me.cache.clear(Kind::Master);
                        let _ = gio::spawn_blocking(|| secret::forget(Kind::Master)).await;
                        error_dialog(&parent2, "Datenbank konnte nicht geöffnet werden", &e);
                    }
                }
            });
        });
    }

    pub fn lock(&self) {
        self.cache.clear(Kind::Master);
        glib::MainContext::default().spawn_local(async {
            let _ = gio::spawn_blocking(|| secret::forget(Kind::Master)).await;
        });
    }

    pub fn status_text(&self) -> String {
        match self.app().map(|a| a.config.borrow().password_manager.clone()).as_deref() {
            Some("keepassxc") => format!("KeePassXC-Datenbank: {}", self.location().map(|l| l.path.display().to_string()).unwrap_or_default()),
            Some("own") => "PoeziBrowse-Passwortmanager".into(),
            Some("off") => "Aus".into(),
            _ => "Noch nicht eingerichtet".into(),
        }
    }

    pub fn button_clicked(self: &Rc<Self>, parent: &gtk::Window, button: &gtk::Button, view: Option<WebView>, mode: Mode) {
        if !self.enabled() {
            self.setup_dialog(parent);
            return;
        }
        let popover = gtk::Popover::new(Some(button));
        popover.style_context().add_class("passwords");
        let bx = gtk::Box::new(Orientation::Vertical, 6);
        for m in [&bx] {
            m.set_margin_top(12);
            m.set_margin_bottom(12);
            m.set_margin_start(12);
            m.set_margin_end(12);
        }
        popover.add(&bx);
        let host = view.as_ref().and_then(Self::secure_host);
        let title = gtk::Label::new(Some(&match &host {
            Some(h) => format!("Logins für {h}"),
            None => "Passwörter".into(),
        }));
        title.set_xalign(0.0);
        title.style_context().add_class("panel-title");
        bx.pack_start(&title, false, false, 0);
        let list = gtk::Box::new(Orientation::Vertical, 4);
        bx.pack_start(&list, false, false, 0);
        if mode == Mode::Tor {
            list.pack_start(&dim_label("Im Tor-Tab ist der Passwortmanager aus, damit dich Logins nicht verraten."), false, false, 0);
        } else if host.is_none() {
            list.pack_start(&dim_label("Ausfüllen geht nur auf HTTPS-Seiten."), false, false, 0);
        } else {
            let spinner = gtk::Spinner::new();
            spinner.start();
            list.pack_start(&spinner, false, false, 6);
        }
        let generate = gtk::Button::with_label("Sicheres Passwort erzeugen");
        let open_manager = gtk::Button::with_label("Passwortmanager öffnen");
        let lock = gtk::Button::with_label("Sperren");
        let row = gtk::Box::new(Orientation::Horizontal, 6);
        row.pack_start(&open_manager, true, true, 0);
        row.pack_start(&lock, false, false, 0);
        if mode != Mode::Tor && host.is_some() {
            bx.pack_start(&generate, false, false, 0);
        }
        bx.pack_start(&row, false, false, 0);
        popover.show_all();
        popover.popup();

        let me = self.clone();
        let parent2 = parent.clone();
        let p = popover.clone();
        open_manager.connect_clicked(move |_| {
            p.popdown();
            me.manager_window(&parent2);
        });
        let me = self.clone();
        let p = popover.clone();
        lock.connect_clicked(move |_| {
            me.lock();
            p.popdown();
        });
        let (Some(host), Some(view)) = (host, view) else { return };
        if mode == Mode::Tor {
            return;
        }
        let me = self.clone();
        let p = popover.clone();
        let v = view.clone();
        let h = host.clone();
        generate.connect_clicked(move |_| {
            p.popdown();
            me.generate_for(&v, &h);
        });
        let me = self.clone();
        let p = popover.clone();
        self.with_logins(parent, move |logins, _| {
            for child in list.children() {
                list.remove(&child);
            }
            let matching: Vec<&Login> = logins.iter().filter(|l| vault::host_matches(&l.url, &host)).collect();
            if matching.is_empty() {
                list.pack_start(&dim_label(&format!("Keine gespeicherten Logins für {host}.")), false, false, 0);
            }
            for login in matching {
                let entry = gtk::Button::new();
                entry.style_context().add_class("login-entry");
                let text = gtk::Box::new(Orientation::Vertical, 1);
                let name = gtk::Label::new(Some(if login.username.is_empty() { &login.title } else { &login.username }));
                name.set_xalign(0.0);
                let detail = gtk::Label::new(Some(&format!("{} · ••••••••", login.title)));
                detail.set_xalign(0.0);
                detail.style_context().add_class("dim-label");
                text.pack_start(&name, false, false, 0);
                text.pack_start(&detail, false, false, 0);
                entry.add(&text);
                entry.set_tooltip_text(Some("Klicken zum Ausfüllen"));
                let me = me.clone();
                let p = p.clone();
                let view = view.clone();
                let host = host.clone();
                let username = login.username.clone();
                let password = login.password.clone();
                entry.connect_clicked(move |_| {
                    p.popdown();
                    me.fill(&view, &host, &username, &password);
                });
                list.pack_start(&entry, false, false, 0);
            }
            list.show_all();
        });
    }

    fn fill(&self, view: &WebView, host: &str, username: &str, password: &str) {
        if Self::secure_host(view).as_deref() != Some(host) {
            return;
        }
        let script = Zeroizing::new(format!(
            "window.__poeziFill({}, {})",
            serde_json::to_string(username).unwrap_or_default(),
            serde_json::to_string(password).unwrap_or_default()
        ));
        view.evaluate_javascript(&script, Some(WORLD), None, None::<&gio::Cancellable>, |_| {});
    }

    fn generate_for(self: &Rc<Self>, view: &WebView, host: &str) {
        let view = view.clone();
        let host = host.to_string();
        glib::MainContext::default().spawn_local(async move {
            let Ok(password) = gio::spawn_blocking(vault::generate_password).await else { return };
            if PasswordManager::secure_host(&view).as_deref() != Some(host.as_str()) {
                return;
            }
            let script = Zeroizing::new(format!("window.__poeziFillNew({})", serde_json::to_string(&*password).unwrap_or_default()));
            view.evaluate_javascript(&script, Some(WORLD), None, None::<&gio::Cancellable>, |_| {});
        });
    }

    fn on_submit(self: &Rc<Self>, view: &WebView, host: String, username: String, password: Zeroizing<String>) {
        let Some(app) = self.app() else { return };
        let Some(window) = app.window_for_view(view) else { return };
        let label = self.label();
        if !self.host_known(&host) {
            let question = format!("Login für {host} in {label} speichern?");
            self.ask_save(&window, question, host, username, password, None);
            return;
        }
        if self.cache.get(Kind::Master).is_none() {
            return;
        }
        let me = self.clone();
        let parent = window.window.clone().upcast::<gtk::Window>();
        self.with_logins(&parent, move |logins, _| {
            let matching: Vec<&Login> = logins.iter().filter(|l| vault::host_matches(&l.url, &host)).collect();
            if let Some(existing) = matching.iter().find(|l| l.username == username) {
                if *existing.password != *password {
                    let question = format!("Passwort für {username} auf {host} in {label} aktualisieren?");
                    let path = existing.entry_path.clone();
                    me.ask_save(&window, question, host, username, password, Some(path));
                }
                return;
            }
            let question = format!("Neuen Login für {host} in {label} speichern?");
            me.ask_save(&window, question, host, username, password, None);
        });
    }

    fn ask_save(
        self: &Rc<Self>,
        window: &Rc<crate::window::BrowserWindow>,
        question: String,
        host: String,
        username: String,
        password: Zeroizing<String>,
        update: Option<String>,
    ) {
        let me = self.clone();
        let parent = window.window.clone().upcast::<gtk::Window>();
        window.show_prompt(&question, "Ja, speichern", "Nein", move |yes| {
            if !yes {
                return;
            }
            let Some(location) = me.location() else { return };
            let me2 = me.clone();
            let parent2 = parent.clone();
            let host = host.clone();
            let username = username.clone();
            let password = password.clone();
            let update = update.clone();
            me.with_master(&parent, move |master| {
                glib::MainContext::default().spawn_local(async move {
                    let host2 = host.clone();
                    let location2 = location.clone();
                    let result = gio::spawn_blocking(move || match update {
                        Some(path) => vault::update_password(&location2, &master, &path, &password),
                        None => vault::add_login(&location2, &master, &host2, &username, &format!("https://{host2}"), &password),
                    })
                    .await
                    .unwrap_or_else(|_| Err("Fehler".into()));
                    match result {
                        Ok(()) => {
                            if let Some(key) = me2.cache.get(Kind::IndexKey) {
                                let mut index = me2.index.borrow_mut();
                                index.hashes.insert(hmac(key.as_bytes(), host.trim_start_matches("www.")));
                                index.state = vault::file_state(&location.path);
                                write_index(&index.hashes, index.state);
                            }
                        }
                        Err(e) => error_dialog(&parent2, "Login konnte nicht gespeichert werden", &e),
                    }
                });
            });
        });
    }

    fn unlock_dialog(self: &Rc<Self>, parent: &gtk::Window, then: Box<dyn FnOnce(Zeroizing<String>)>) {
        let Some(location) = self.location() else { return };
        let dialog = gtk::Dialog::with_buttons(
            Some(&format!("{} entsperren", self.label())),
            Some(parent),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[("Abbrechen", gtk::ResponseType::Cancel), ("Entsperren", gtk::ResponseType::Accept)],
        );
        dialog.set_default_response(gtk::ResponseType::Accept);
        let area = dialog.content_area();
        area.set_spacing(10);
        for m in [&area] {
            m.set_margin_top(14);
            m.set_margin_bottom(8);
            m.set_margin_start(16);
            m.set_margin_end(16);
        }
        let info = gtk::Label::new(Some(&format!(
            "Master-Passwort für\n{}\n\nPoeziBrowse merkt es sich danach 7 Tage lang sicher und fragt dann neu.",
            location.path.display()
        )));
        info.set_xalign(0.0);
        info.set_line_wrap(true);
        let entry = password_entry();
        let error = gtk::Label::new(None);
        error.style_context().add_class("error");
        area.pack_start(&info, false, false, 0);
        area.pack_start(&entry, false, false, 0);
        area.pack_start(&error, false, false, 0);
        let then = Rc::new(RefCell::new(Some(then)));
        let me = self.clone();
        dialog.connect_response(move |dialog, response| {
            if response != gtk::ResponseType::Accept {
                dialog.close();
                return;
            }
            let password = Zeroizing::new(entry.text().to_string());
            entry.set_text("");
            dialog.set_sensitive(false);
            let me = me.clone();
            let dialog = dialog.clone();
            let error = error.clone();
            let then = then.clone();
            let location = location.clone();
            glib::MainContext::default().spawn_local(async move {
                let check = password.clone();
                let ok = gio::spawn_blocking(move || vault::open(&location, &check).map(|_| ())).await.unwrap_or_else(|_| Err("Fehler".into()));
                dialog.set_sensitive(true);
                match ok {
                    Ok(()) => {
                        me.cache.put(Kind::Master, &password);
                        let stored = password.clone();
                        let _ = gio::spawn_blocking(move || secret::store(Kind::Master, &stored)).await;
                        dialog.close();
                        if let Some(then) = then.borrow_mut().take() {
                            then(password);
                        }
                    }
                    Err(e) => error.set_text(&e),
                }
            });
        });
        dialog.show_all();
    }

    pub fn setup_dialog(self: &Rc<Self>, parent: &gtk::Window) {
        let dialog = gtk::Dialog::with_buttons(
            Some("Passwortmanager einrichten"),
            Some(parent),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[("Später", gtk::ResponseType::Cancel), ("Einrichten", gtk::ResponseType::Accept)],
        );
        dialog.set_default_response(gtk::ResponseType::Accept);
        dialog.set_default_width(520);
        let area = dialog.content_area();
        area.set_spacing(10);
        for m in [&area] {
            m.set_margin_top(14);
            m.set_margin_bottom(8);
            m.set_margin_start(18);
            m.set_margin_end(18);
        }
        let intro = gtk::Label::new(Some("Soll PoeziBrowse Logins ausfüllen und speichern können?"));
        intro.set_xalign(0.0);
        intro.style_context().add_class("panel-title");
        let keepass = gtk::RadioButton::with_label("KeePassXC-Datenbank verwenden (.kdbx)");
        let own = gtk::RadioButton::with_label_from_widget(&keepass, "PoeziBrowse-Passwortmanager (neue Datenbank, mit KeePassXC kompatibel)");
        let none = gtk::RadioButton::with_label_from_widget(&keepass, "Keinen Passwortmanager");
        let file = gtk::FileChooserButton::new("KeePassXC-Datenbank wählen", gtk::FileChooserAction::Open);
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("KeePass-Datenbank"));
        filter.add_pattern("*.kdbx");
        file.add_filter(filter);
        let keyfile = gtk::FileChooserButton::new("Schlüsseldatei (optional)", gtk::FileChooserAction::Open);
        let keyfile_label = dim_label("Schlüsseldatei, falls deine Datenbank eine braucht (optional):");
        let password = password_entry();
        password.set_placeholder_text(Some("Master-Passwort"));
        let repeat = password_entry();
        repeat.set_placeholder_text(Some("Master-Passwort wiederholen"));
        let error = gtk::Label::new(None);
        error.style_context().add_class("error");
        error.set_line_wrap(true);
        error.set_xalign(0.0);
        let note = dim_label(
            "PoeziBrowse liest die Datenbank nur, wenn eine Seite ein Login-Formular hat, und füllt erst nach deinem Klick aus. \
             Gespeichert wird nur, wenn du „Ja“ drückst – über KeePassXC selbst (keepassxc-cli). Das Master-Passwort wird mit dem \
             TPM-Chip deines PCs verschlüsselt (oder im Schlüsselbund) und nach 7 Tagen neu abgefragt. Im Klartext wird es nie gespeichert.",
        );
        area.pack_start(&intro, false, false, 0);
        area.pack_start(&keepass, false, false, 0);
        area.pack_start(&file, false, false, 0);
        area.pack_start(&keyfile_label, false, false, 0);
        area.pack_start(&keyfile, false, false, 0);
        area.pack_start(&own, false, false, 0);
        area.pack_start(&none, false, false, 0);
        area.pack_start(&password, false, false, 4);
        area.pack_start(&repeat, false, false, 0);
        area.pack_start(&error, false, false, 0);
        area.pack_start(&note, false, false, 6);
        if let Some(app) = self.app() {
            let c = app.config.borrow();
            match c.password_manager.as_str() {
                "own" => own.set_active(true),
                "off" => none.set_active(true),
                _ => keepass.set_active(true),
            }
            if !c.pm_database.is_empty() {
                file.set_filename(&c.pm_database);
            }
            if !c.pm_keyfile.is_empty() {
                keyfile.set_filename(&c.pm_keyfile);
            }
        }
        let update = {
            let (keepass, own, file, keyfile, keyfile_label, password, repeat) =
                (keepass.clone(), own.clone(), file.clone(), keyfile.clone(), keyfile_label.clone(), password.clone(), repeat.clone());
            move || {
                let k = keepass.is_active();
                let o = own.is_active();
                file.set_sensitive(k);
                keyfile.set_sensitive(k);
                keyfile_label.set_sensitive(k);
                password.set_visible(k || o);
                repeat.set_visible(o);
            }
        };
        for radio in [&keepass, &own, &none] {
            let update = update.clone();
            radio.connect_toggled(move |_| update());
        }
        dialog.show_all();
        update();
        let me = self.clone();
        dialog.connect_response(move |dialog, response| {
            let Some(app) = me.app() else { return };
            if response != gtk::ResponseType::Accept {
                if app.config.borrow().password_manager.is_empty() {
                    app.set_layout_option(|c| c.password_manager = "off".into());
                }
                dialog.close();
                return;
            }
            if none.is_active() {
                me.lock();
                app.set_layout_option(|c| c.password_manager = "off".into());
                dialog.close();
                return;
            }
            let kind = if keepass.is_active() { "keepassxc" } else { "own" };
            let pass = Zeroizing::new(password.text().to_string());
            if pass.is_empty() {
                error.set_text("Bitte das Master-Passwort eingeben.");
                return;
            }
            let location = if kind == "keepassxc" {
                let Some(path) = file.filename() else {
                    error.set_text("Bitte die KeePassXC-Datenbank auswählen.");
                    return;
                };
                Location { path, keyfile: keyfile.filename() }
            } else {
                if *pass != *repeat.text() {
                    error.set_text("Die Passwörter stimmen nicht überein.");
                    return;
                }
                if pass.chars().count() < 10 {
                    error.set_text("Bitte mindestens 10 Zeichen verwenden.");
                    return;
                }
                Location { path: secret::dir().join("poezibrowse.kdbx"), keyfile: None }
            };
            password.set_text("");
            repeat.set_text("");
            dialog.set_sensitive(false);
            let me = me.clone();
            let dialog = dialog.clone();
            let error = error.clone();
            glib::MainContext::default().spawn_local(async move {
                let loc = location.clone();
                let check = pass.clone();
                let create = kind == "own" && !location.path.is_file();
                let result = gio::spawn_blocking(move || {
                    if create {
                        vault::create_database(&loc, &check)?;
                    }
                    let logins = vault::open(&loc, &check)?;
                    let key = get_or_create_index_key();
                    let hashes = index_hashes(key.as_bytes(), &logins);
                    let state = vault::file_state(&loc.path);
                    write_index(&hashes, state);
                    secret::forget(Kind::Master);
                    let store = secret::store(Kind::Master, &check);
                    Ok::<_, String>((hashes, state, key, store, logins.len()))
                })
                .await
                .unwrap_or_else(|_| Err("Fehler".into()));
                dialog.set_sensitive(true);
                match result {
                    Ok((hashes, state, key, store, count)) => {
                        me.cache.put(Kind::Master, &pass);
                        me.cache.put(Kind::IndexKey, &key);
                        *me.index.borrow_mut() = Index { hashes, state, loaded: true };
                        if let Some(app) = me.app() {
                            let path = location.path.to_string_lossy().to_string();
                            let keyfile = location.keyfile.as_ref().map(|k| k.to_string_lossy().to_string()).unwrap_or_default();
                            app.set_layout_option(|c| {
                                c.password_manager = kind.into();
                                if kind == "keepassxc" {
                                    c.pm_database = path;
                                    c.pm_keyfile = keyfile;
                                }
                            });
                            app.password_state_changed();
                        }
                        let parent = dialog.transient_for();
                        dialog.close();
                        if let Some(parent) = parent {
                            info_dialog(
                                &parent,
                                "Passwortmanager eingerichtet",
                                &format!("{count} Einträge gefunden.\n\nDas Master-Passwort liegt jetzt {}.", store.describe()),
                            );
                        }
                    }
                    Err(e) => error.set_text(&e),
                }
            });
        });
    }

    pub fn manager_window(self: &Rc<Self>, parent: &gtk::Window) {
        if !self.enabled() {
            self.setup_dialog(parent);
            return;
        }
        let me = self.clone();
        let parent2 = parent.clone();
        self.with_logins(parent, move |logins, _| {
            let window = gtk::Window::new(gtk::WindowType::Toplevel);
            window.set_title(&format!("Passwörter – {}", me.label()));
            window.set_transient_for(Some(&parent2));
            window.set_default_size(720, 560);
            window.style_context().add_class("password-manager");
            let root = gtk::Box::new(Orientation::Vertical, 8);
            for m in [&root] {
                m.set_margin_top(12);
                m.set_margin_bottom(12);
                m.set_margin_start(12);
                m.set_margin_end(12);
            }
            let search = gtk::SearchEntry::new();
            search.set_placeholder_text(Some("Einträge durchsuchen …"));
            let list = gtk::ListBox::new();
            list.set_selection_mode(gtk::SelectionMode::None);
            let scroller = gtk::ScrolledWindow::builder().vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).build();
            scroller.add(&list);
            let footer = gtk::Box::new(Orientation::Horizontal, 8);
            let status = dim_label(&format!("{} · {} Einträge · Passwörter werden nie angezeigt, nur kopiert", me.status_text(), logins.len()));
            status.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            let lock = gtk::Button::with_label("Sperren");
            let change = gtk::Button::with_label("Datenbank ändern …");
            footer.pack_start(&status, true, true, 0);
            footer.pack_end(&lock, false, false, 0);
            footer.pack_end(&change, false, false, 0);
            root.pack_start(&search, false, false, 0);
            root.pack_start(&scroller, true, true, 0);
            root.pack_start(&footer, false, false, 0);
            window.add(&root);

            let logins = Rc::new(logins);
            let fill_list = {
                let list = list.clone();
                let logins = logins.clone();
                let window = window.clone();
                move |query: &str| {
                    for child in list.children() {
                        list.remove(&child);
                    }
                    let query = query.to_lowercase();
                    for (i, login) in logins.iter().enumerate() {
                        let haystack = format!("{} {} {}", login.title, login.username, login.url).to_lowercase();
                        if !query.is_empty() && !haystack.contains(&query) {
                            continue;
                        }
                        list.add(&login_row(&window, &logins, i));
                    }
                    list.show_all();
                }
            };
            fill_list("");
            let f = fill_list.clone();
            search.connect_search_changed(move |s| f(&s.text()));
            let me2 = me.clone();
            let w = window.clone();
            lock.connect_clicked(move |_| {
                me2.lock();
                w.close();
            });
            let me2 = me.clone();
            let w = window.clone();
            let p = parent2.clone();
            change.connect_clicked(move |_| {
                w.close();
                me2.setup_dialog(&p);
            });
            window.show_all();
        });
    }
}

fn login_row(window: &gtk::Window, logins: &Rc<Vec<Login>>, index: usize) -> gtk::ListBoxRow {
    let login = &logins[index];
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let bx = gtk::Box::new(Orientation::Horizontal, 10);
    for m in [&bx] {
        m.set_margin_top(6);
        m.set_margin_bottom(6);
        m.set_margin_start(6);
        m.set_margin_end(6);
    }
    let text = gtk::Box::new(Orientation::Vertical, 2);
    let title = gtk::Label::new(Some(if login.title.is_empty() { "(ohne Titel)" } else { &login.title }));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let detail = gtk::Label::new(Some(&format!("{} · ••••••••  {}", login.username, login.url)));
    detail.set_xalign(0.0);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    detail.style_context().add_class("dim-label");
    text.pack_start(&title, false, false, 0);
    text.pack_start(&detail, false, false, 0);
    let copy_user = gtk::Button::with_label("Benutzer kopieren");
    let copy_password = gtk::Button::with_label("Passwort kopieren");
    bx.pack_start(&text, true, true, 0);
    bx.pack_end(&copy_password, false, false, 0);
    bx.pack_end(&copy_user, false, false, 0);
    row.add(&bx);
    let username = login.username.clone();
    copy_user.connect_clicked(move |_| copy_to_clipboard(&username, false));
    let logins = logins.clone();
    let w = window.clone();
    copy_password.connect_clicked(move |button| {
        copy_to_clipboard(&logins[index].password, true);
        button.set_label("Kopiert – wird in 30 s gelöscht");
        let b = button.clone();
        glib::timeout_add_local_once(Duration::from_secs(3), move || b.set_label("Passwort kopieren"));
        let _ = &w;
    });
    row
}

fn copy_to_clipboard(text: &str, secret: bool) {
    let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clipboard.set_text(text);
    if !secret {
        return;
    }
    let expected = Zeroizing::new(text.to_string());
    glib::timeout_add_local_once(Duration::from_secs(30), move || {
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        if clipboard.wait_for_text().is_some_and(|t| *t == **expected) {
            clipboard.set_text("");
        }
    });
}

fn password_entry() -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_visibility(false);
    entry.set_input_purpose(gtk::InputPurpose::Password);
    entry.set_activates_default(true);
    entry
}

fn dim_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_max_width_chars(60);
    label.style_context().add_class("dim-label");
    label
}

fn error_dialog(parent: &gtk::Window, title: &str, message: &str) {
    let dialog = gtk::MessageDialog::new(Some(parent), gtk::DialogFlags::MODAL, gtk::MessageType::Error, gtk::ButtonsType::Ok, title);
    dialog.set_secondary_text(Some(message));
    dialog.connect_response(|d, _| d.close());
    dialog.show_all();
}

fn info_dialog(parent: &gtk::Window, title: &str, message: &str) {
    let dialog = gtk::MessageDialog::new(Some(parent), gtk::DialogFlags::MODAL, gtk::MessageType::Info, gtk::ButtonsType::Ok, title);
    dialog.set_secondary_text(Some(message));
    dialog.connect_response(|d, _| d.close());
    dialog.show_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_hmac_is_keyed() {
        let a = hmac(b"key-one", "github.com");
        let b = hmac(b"key-two", "github.com");
        assert_ne!(a, b);
        assert_eq!(a, hmac(b"key-one", "github.com"));
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0u8, 1, 0xab, 0xff];
        assert_eq!(unhex(&hex(&bytes)).unwrap(), bytes);
    }
}
