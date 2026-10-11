use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use webkit2gtk::{Download, DownloadExt, URIResponseExt, WebContextExt};

use crate::blocker::Blocker;
use crate::config::{Config, EngineKind};
use crate::engine::webkit::{Blocking, PageHandle, WebKitEngine};
use crate::engine::Page;
use crate::session::{self, Session, TabState, WindowState};
use crate::tab::Snapshot;
use crate::window::BrowserWindow;
use crate::storage::Storage;
use crate::{memory, pages, passwords};

pub struct App {
    pub gtk_app: gtk::Application,
    pub config: RefCell<Config>,
    webkit: RefCell<Option<Rc<WebKitEngine>>>,
    webkit_private: RefCell<Option<Rc<WebKitEngine>>>,
    blocker: RefCell<Option<Rc<Blocker>>>,
    pub storage: Storage,
    pub token: String,
    windows: RefCell<Vec<Rc<BrowserWindow>>>,
    next_tab_id: Cell<u64>,
    session_dirty: Cell<bool>,
    quitting: Cell<bool>,
    me: Weak<App>,
}

impl App {
    pub fn new(gtk_app: &gtk::Application) -> Rc<App> {
        Rc::new_cyclic(|me| App {
            gtk_app: gtk_app.clone(),
            config: RefCell::new(Config::load()),
            webkit: RefCell::new(None),
            webkit_private: RefCell::new(None),
            blocker: RefCell::new(None),
            storage: Storage::open(),
            token: random_token(),
            windows: RefCell::new(Vec::new()),
            next_tab_id: Cell::new(1),
            session_dirty: Cell::new(false),
            quitting: Cell::new(false),
            me: me.clone(),
        })
    }

    pub fn next_tab_id(&self) -> u64 {
        let id = self.next_tab_id.get();
        self.next_tab_id.set(id + 1);
        id
    }

    pub fn start(self: &Rc<Self>) {
        load_css();
        if self.config.borrow().block_ads {
            let blocker = Blocker::new();
            blocker.start();
            *self.blocker.borrow_mut() = Some(blocker);
        }
        self.start_timers();
        let app = self.me.clone();
        self.gtk_app.connect_shutdown(move |_| {
            if let Some(app) = app.upgrade() {
                app.quitting.set(true);
                app.save_session();
            }
        });
    }

    fn webkit_engine(&self, private: bool) -> Rc<WebKitEngine> {
        let slot = if private { &self.webkit_private } else { &self.webkit };
        if let Some(engine) = slot.borrow().as_ref() {
            return engine.clone();
        }
        let me = self.me.clone();
        let handler: crate::engine::webkit::SchemeHandler = Rc::new(move |uri: &str| match me.upgrade() {
            Some(app) => pages::handle(uri, &app),
            None => ("text/plain".into(), Vec::new()),
        });
        let engine = WebKitEngine::new(&self.config.borrow(), private, handler);
        engine.set_content_manager_hook(Rc::new(move |ucm| passwords::setup_page(ucm, private)));
        let me = self.me.clone();
        engine.context().connect_download_started(move |_, download| {
            if let Some(app) = me.upgrade() {
                app.download_started(download);
            }
        });
        if let Some(blocker) = self.blocker.borrow().clone() {
            let me = self.me.clone();
            engine.set_blocking(Blocking {
                blocker,
                enabled_for: Rc::new(move |host| me.upgrade().is_some_and(|app| !app.config.borrow().is_allowlisted(host))),
            });
        }
        *slot.borrow_mut() = Some(engine.clone());
        engine
    }

    pub fn create_page(&self, engine: EngineKind, private: bool) -> Box<dyn Page> {
        match engine {
            #[cfg(feature = "servo")]
            EngineKind::Servo => crate::engine::servo::create_page(&self.config.borrow(), private),
            _ => Box::new(PageHandle(self.webkit_engine(private).create_page(None))),
        }
    }

    pub fn new_window(self: &Rc<Self>, private: bool, with_home: bool) -> Rc<BrowserWindow> {
        let window = BrowserWindow::new(self, private);
        self.windows.borrow_mut().push(window.clone());
        if with_home {
            let home = self.config.borrow().homepage.clone();
            window.open_tab(&home, false);
        }
        window.window.present();
        window
    }

    pub fn window_closing(&self, window: &Rc<BrowserWindow>) {
        let remaining = self.windows.borrow().iter().filter(|w| !w.private && !Rc::ptr_eq(w, window)).count();
        if remaining > 0 || window.private {
            self.windows.borrow_mut().retain(|w| !Rc::ptr_eq(w, window));
            self.save_session();
        } else {
            self.save_session();
            self.windows.borrow_mut().retain(|w| !Rc::ptr_eq(w, window));
        }
    }

    fn last_window(&self) -> Option<Rc<BrowserWindow>> {
        let active = self.gtk_app.active_window();
        let windows = self.windows.borrow();
        active
            .and_then(|a| windows.iter().find(|w| w.window.upcast_ref::<gtk::Window>() == &a).cloned())
            .or_else(|| windows.iter().rev().find(|w| !w.private).cloned())
            .or_else(|| windows.last().cloned())
    }

    pub fn activate(self: &Rc<Self>) {
        if let Some(window) = self.last_window() {
            window.window.present();
            return;
        }
        if !self.restore_session() {
            self.new_window(false, true);
        }
    }

    pub fn open_uris(self: &Rc<Self>, uris: &[String]) {
        let window = match self.last_window() {
            Some(w) => w,
            None => {
                let restored = self.restore_session();
                match self.last_window() {
                    Some(w) if restored => w,
                    _ => self.new_window(false, false),
                }
            }
        };
        for uri in uris {
            window.open_tab(uri, false);
        }
        window.window.present();
    }

    pub fn session_changed(&self) {
        self.session_dirty.set(true);
    }

    fn save_session(&self) {
        if !self.config.borrow().restore_session {
            return;
        }
        let windows = self.windows.borrow();
        let mut session = Session::default();
        for window in windows.iter().filter(|w| !w.private) {
            let current = window.current_tab().map(|t| t.id);
            let mut state = WindowState::default();
            for (i, tab) in window.tabs().iter().enumerate() {
                let snapshot = match tab.page.borrow().as_ref() {
                    Some(page) => Snapshot {
                        uri: page.uri().unwrap_or_default(),
                        title: page.title().unwrap_or_default(),
                        engine: page.engine(),
                        state: page.save_state(),
                    },
                    None => match tab.snapshot.borrow().clone() {
                        Some(s) => s,
                        None => continue,
                    },
                };
                if snapshot.uri.is_empty() {
                    continue;
                }
                if Some(tab.id) == current {
                    state.active = i;
                }
                state.tabs.push(TabState::from_snapshot(&snapshot));
            }
            if !state.tabs.is_empty() {
                session.windows.push(state);
            }
        }
        if session.windows.is_empty() && self.quitting.get() {
            return;
        }
        session::save(&session);
        self.session_dirty.set(false);
    }

    fn restore_session(self: &Rc<Self>) -> bool {
        if !self.config.borrow().restore_session {
            return false;
        }
        let session = session::load();
        if session.windows.is_empty() {
            return false;
        }
        for state in session.windows {
            let window = self.new_window(false, false);
            let active = state.active;
            let mut tabs = Vec::new();
            for tab in state.tabs {
                tabs.push(window.add_sleeping_tab(tab.into_snapshot()));
            }
            if let Some(tab) = tabs.get(active).or(tabs.first()) {
                window.wake(tab);
                window.update_toolbar();
                if let Some(n) = window_page_num(&window, tab) {
                    window.select_page(n);
                }
            }
        }
        true
    }

    pub fn page_visited(&self, uri: &str, title: &str) {
        self.storage.record_visit(uri, title);
    }

    pub fn blocker_rules(&self) -> Option<bool> {
        self.blocker.borrow().as_ref().map(|b| !b.filters.borrow().is_empty())
    }

    fn enforce_ram_budget(&self, used: u64) {
        let budget = self.config.borrow().ram_budget_mb as u64 * 1024 * 1024;
        if budget == 0 || used <= budget {
            return;
        }
        let candidate = self
            .windows
            .borrow()
            .iter()
            .flat_map(|w| w.sleep_candidates().into_iter().map(move |t| (w.clone(), t)))
            .min_by_key(|(_, t)| t.last_active.get());
        if let Some((window, tab)) = candidate {
            window.put_to_sleep(&tab);
        }
    }

    fn start_timers(self: &Rc<Self>) {
        let me = self.me.clone();
        glib::timeout_add_local(Duration::from_secs(30), move || {
            let Some(app) = me.upgrade() else { return glib::ControlFlow::Break };
            let minutes = app.config.borrow().tab_sleep_minutes;
            if minutes > 0 {
                for window in app.windows.borrow().clone() {
                    window.sleep_idle_tabs(minutes);
                }
            }
            if app.session_dirty.get() {
                app.save_session();
            }
            glib::ControlFlow::Continue
        });

        let me = self.me.clone();
        let tick = move || {
            let Some(app) = me.upgrade() else { return glib::ControlFlow::Break };
            let me = app.me.clone();
            glib::MainContext::default().spawn_local(async move {
                let Ok(bytes) = gio::spawn_blocking(memory::total_usage).await else { return };
                if let Some(app) = me.upgrade() {
                    app.enforce_ram_budget(bytes);
                    let text = format!("RAM {}", memory::format_bytes(bytes));
                    for window in app.windows.borrow().iter() {
                        window.ram_label.set_text(&text);
                    }
                }
            });
            glib::ControlFlow::Continue
        };
        tick();
        glib::timeout_add_local(Duration::from_secs(3), tick);
    }

    fn download_started(self: &Rc<Self>, download: &Download) {
        let Some(window) = self.last_window() else { return };
        let dir = self.config.borrow().download_dir();

        let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
        row.set_margin_start(10);
        row.set_margin_end(10);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        let name = gtk::Label::new(Some("Download wird vorbereitet …"));
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let progress = gtk::ProgressBar::new();
        let info = gtk::Label::new(None);
        info.set_xalign(0.0);
        info.style_context().add_class("dim-label");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let cancel = gtk::Button::with_label("Abbrechen");
        let open = gtk::Button::with_label("Öffnen");
        let folder = gtk::Button::with_label("Ordner öffnen");
        open.set_no_show_all(true);
        folder.set_no_show_all(true);
        buttons.pack_end(&cancel, false, false, 0);
        buttons.pack_end(&folder, false, false, 0);
        buttons.pack_end(&open, false, false, 0);
        row.add(&name);
        row.add(&progress);
        row.add(&info);
        row.add(&buttons);
        row.show_all();
        window.downloads.prepend(&row);
        window.show_downloads();

        let target: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));
        let t = target.clone();
        let n = name.clone();
        download.connect_decide_destination(move |download, suggested| {
            let path = unique_path(&dir, suggested);
            n.set_text(&path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default());
            download.set_destination(&glib::filename_to_uri(&path, None).map(|u| u.to_string()).unwrap_or_default());
            *t.borrow_mut() = Some(path);
            true
        });
        let p = progress.clone();
        let i = info.clone();
        download.connect_estimated_progress_notify(move |download| {
            p.set_fraction(download.estimated_progress());
            let received = download.received_data_length();
            let total = download.response().map(|r| r.content_length()).unwrap_or(0);
            i.set_text(&if total > 0 {
                format!("{} von {}", memory::format_bytes(received), memory::format_bytes(total))
            } else {
                memory::format_bytes(received)
            });
        });
        let d = download.clone();
        cancel.connect_clicked(move |_| d.cancel());
        let (c, o, f, i, p) = (cancel.clone(), open.clone(), folder.clone(), info.clone(), progress.clone());
        download.connect_finished(move |_| {
            c.hide();
            o.show();
            f.show();
            p.set_fraction(1.0);
            if !i.text().starts_with("Fehler") {
                i.set_text("Fertig");
            }
        });
        let (i, c) = (info.clone(), cancel.clone());
        download.connect_failed(move |_, error| {
            i.set_text(&format!("Fehler: {error}"));
            c.hide();
        });
        let t = target.clone();
        open.connect_clicked(move |_| {
            if let Some(path) = t.borrow().as_ref() {
                let _ = gio::AppInfo::launch_default_for_uri(&glib::filename_to_uri(path, None).unwrap_or_default(), None::<&gio::AppLaunchContext>);
            }
        });
        let t = target;
        folder.connect_clicked(move |_| {
            if let Some(dir) = t.borrow().as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                let _ = gio::AppInfo::launch_default_for_uri(&glib::filename_to_uri(&dir, None).unwrap_or_default(), None::<&gio::AppLaunchContext>);
            }
        });
    }
}

fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i ^ std::process::id() as u64);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

fn window_page_num(window: &Rc<BrowserWindow>, tab: &Rc<crate::tab::Tab>) -> Option<u32> {
    window.tabs().iter().position(|t| t.id == tab.id).map(|n| n as u32)
}

fn unique_path(dir: &std::path::Path, suggested: &str) -> std::path::PathBuf {
    let name = std::path::Path::new(suggested).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "download".into());
    let name = if name.is_empty() || name == "." || name == ".." { "download".to_string() } else { name };
    let _ = std::fs::create_dir_all(dir);
    let candidate = dir.join(&name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

fn load_css() {
    let css = gtk::CssProvider::new();
    let _ = css.load_from_data(
        br#"
        .sleeping { opacity: 0.55; }
        .sleeping label { font-style: italic; }
        headerbar.private { background-image: none; background-color: #3b2366; color: #fff; }
        headerbar.private entry { background-color: rgba(255,255,255,0.12); color: #fff; }
        .link-status { background-color: @theme_bg_color; border: 1px solid @borders;
                       border-radius: 0 6px 0 0; padding: 2px 8px; font-size: 0.9em; }
        notebook > header.top > tabs > tab { padding: 2px 6px; min-height: 0; }
        notebook > header.top > tabs > tab button { padding: 0; min-height: 18px; min-width: 18px; }
        "#,
    );
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(&screen, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}
