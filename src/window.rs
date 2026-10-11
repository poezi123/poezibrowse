use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Align, Orientation};

use crate::app::App;
use crate::config::{self, EngineKind};
use crate::engine::{Page, PageEvent};
use crate::tab::{Snapshot, Tab};
use crate::titlebar::TitleBar;

pub struct BrowserWindow {
    app: Rc<App>,
    pub window: gtk::ApplicationWindow,
    pub private: bool,
    pub titlebar: TitleBar,
    back: gtk::Button,
    forward: gtk::Button,
    reload: gtk::Button,
    entry: gtk::Entry,
    notebook: gtk::Notebook,
    status: gtk::Label,
    find_revealer: gtk::Revealer,
    find_entry: gtk::SearchEntry,
    pub ram_label: gtk::Label,
    pub downloads: gtk::ListBox,
    downloads_button: gtk::MenuButton,
    password_button: gtk::Button,
    suggestions: gtk::ListStore,
    tabs: RefCell<Vec<Rc<Tab>>>,
    closed: RefCell<Vec<Snapshot>>,
    fullscreen: Cell<bool>,
}

impl BrowserWindow {
    pub fn new(app: &Rc<App>, private: bool) -> Rc<BrowserWindow> {
        let window = gtk::ApplicationWindow::new(&app.gtk_app);
        window.set_default_size(1280, 820);
        window.set_icon_name(Some(config::APP_ID));

        let header = gtk::HeaderBar::new();
        header.set_show_close_button(true);

        let nav = gtk::Box::new(Orientation::Horizontal, 0);
        nav.style_context().add_class("linked");
        let back = icon_button("go-previous-symbolic", "Zurück (Alt+←)");
        let forward = icon_button("go-next-symbolic", "Vor (Alt+→)");
        nav.add(&back);
        nav.add(&forward);
        let reload = icon_button("view-refresh-symbolic", "Neu laden (Strg+R)");
        header.pack_start(&nav);
        header.pack_start(&reload);
        if private {
            let badge = gtk::Label::new(Some("Privat"));
            badge.style_context().add_class("private-badge");
            badge.set_valign(Align::Center);
            header.pack_start(&badge);
        }

        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_width_chars(40);
        entry.set_max_width_chars(110);
        entry.set_placeholder_text(Some(if private { "Privat suchen oder Adresse eingeben" } else { "Suchen oder Adresse eingeben" }));
        entry.set_input_purpose(gtk::InputPurpose::Url);
        header.set_custom_title(Some(&entry));

        let suggestions = gtk::ListStore::new(&[glib::Type::STRING, glib::Type::STRING]);
        let completion = gtk::EntryCompletion::new();
        completion.set_model(Some(&suggestions));
        completion.set_match_func(|_, _, _| true);
        completion.set_minimum_key_length(1);
        completion.set_popup_set_width(true);
        let cell = gtk::CellRendererText::new();
        cell.set_ellipsize(gtk::pango::EllipsizeMode::End);
        completion.pack_start(&cell, true);
        completion.add_attribute(&cell, "markup", 0);
        entry.set_completion(Some(&completion));

        let menu_button = gtk::MenuButton::new();
        menu_button.set_image(Some(&gtk::Image::from_icon_name(Some("open-menu-symbolic"), gtk::IconSize::Button)));
        menu_button.set_menu_model(Some(&main_menu(private)));
        menu_button.set_tooltip_text(Some("Menü"));
        header.pack_end(&menu_button);

        let downloads = gtk::ListBox::new();
        downloads.set_selection_mode(gtk::SelectionMode::None);
        let placeholder = gtk::Label::new(Some("Noch keine Downloads"));
        placeholder.set_margin_top(18);
        placeholder.set_margin_bottom(18);
        placeholder.show();
        downloads.set_placeholder(Some(&placeholder));
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .min_content_width(340)
            .build();
        scroller.add(&downloads);
        scroller.show_all();
        let popover = gtk::Popover::new(None::<&gtk::Widget>);
        popover.add(&scroller);
        let downloads_button = gtk::MenuButton::new();
        downloads_button.set_image(Some(&gtk::Image::from_icon_name(Some("folder-download-symbolic"), gtk::IconSize::Button)));
        downloads_button.set_popover(Some(&popover));
        downloads_button.set_tooltip_text(Some("Downloads"));
        header.pack_end(&downloads_button);

        let password_button = icon_button("dialog-password-symbolic", "Passwörter");
        header.pack_end(&password_button);

        window.set_titlebar(Some(&header));
        let titlebar = TitleBar::install(&header, &app.config.borrow(), private);
        window.style_context().add_class("poezi");

        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_show_border(false);
        notebook.style_context().add_class("poezi");
        notebook.set_group_name(Some("poezibrowse-tabs"));
        let end = gtk::Box::new(Orientation::Horizontal, 6);
        let ram_label = gtk::Label::new(None);
        ram_label.style_context().add_class("dim-label");
        ram_label.set_tooltip_text(Some("Arbeitsspeicher von PoeziBrowse insgesamt (alle Prozesse)"));
        let new_tab = icon_button("tab-new-symbolic", "Neuer Tab (Strg+T)");
        new_tab.set_relief(gtk::ReliefStyle::None);
        new_tab.set_action_name(Some("win.new-tab"));
        end.pack_start(&ram_label, false, false, 0);
        end.pack_start(&new_tab, false, false, 0);
        end.show_all();
        notebook.set_action_widget(&end, gtk::PackType::End);

        let status = gtk::Label::new(None);
        status.set_halign(Align::Start);
        status.set_valign(Align::End);
        status.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        status.set_max_width_chars(90);
        status.style_context().add_class("link-status");
        status.set_no_show_all(true);
        let overlay = gtk::Overlay::new();
        overlay.add(&notebook);
        overlay.add_overlay(&status);
        overlay.set_overlay_pass_through(&status, true);

        let find_entry = gtk::SearchEntry::new();
        find_entry.set_width_chars(30);
        let find_prev = icon_button("go-up-symbolic", "Vorheriger Treffer (Umschalt+Enter)");
        let find_next = icon_button("go-down-symbolic", "Nächster Treffer (Enter)");
        let find_close = icon_button("window-close-symbolic", "Schließen (Esc)");
        find_close.set_relief(gtk::ReliefStyle::None);
        let find_box = gtk::Box::new(Orientation::Horizontal, 6);
        find_box.set_margin_start(8);
        find_box.set_margin_end(8);
        find_box.set_margin_top(6);
        find_box.set_margin_bottom(6);
        let linked = gtk::Box::new(Orientation::Horizontal, 0);
        linked.style_context().add_class("linked");
        linked.add(&find_entry);
        linked.add(&find_prev);
        linked.add(&find_next);
        find_box.pack_start(&linked, false, false, 0);
        find_box.pack_end(&find_close, false, false, 0);
        find_box.style_context().add_class("findbar");
        let find_revealer = gtk::Revealer::new();
        find_revealer.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        find_revealer.add(&find_box);

        let root = gtk::Box::new(Orientation::Vertical, 0);
        root.pack_start(&overlay, true, true, 0);
        root.pack_start(&find_revealer, false, false, 0);
        window.add(&root);

        let this = Rc::new(BrowserWindow {
            app: app.clone(),
            window,
            private,
            titlebar,
            back,
            forward,
            reload,
            entry,
            notebook,
            status,
            find_revealer,
            find_entry,
            ram_label,
            downloads,
            downloads_button,
            password_button,
            suggestions,
            tabs: RefCell::new(Vec::new()),
            closed: RefCell::new(Vec::new()),
            fullscreen: Cell::new(false),
        });
        this.connect_ui(&find_prev, &find_next, &find_close);
        this.add_actions();
        this.window.show_all();
        this
    }

    fn weak(self: &Rc<Self>) -> Weak<Self> {
        Rc::downgrade(self)
    }

    fn connect_ui(self: &Rc<Self>, find_prev: &gtk::Button, find_next: &gtk::Button, find_close: &gtk::Button) {
        let w = self.weak();
        self.back.connect_clicked(move |_| with(&w, |w| w.with_page(|p| p.go_back())));
        let w = self.weak();
        self.forward.connect_clicked(move |_| with(&w, |w| w.with_page(|p| p.go_forward())));
        let w = self.weak();
        self.reload.connect_clicked(move |_| {
            with(&w, |w| {
                w.with_page(|p| if p.is_loading() { p.stop() } else { p.reload(false) })
            })
        });

        let w = self.weak();
        self.entry.connect_activate(move |entry| {
            with(&w, |w| {
                let uri = config::resolve_input(&entry.text(), &w.app.config.borrow());
                w.load_in_current(&uri);
            })
        });
        let w = self.weak();
        self.entry.connect_key_press_event(move |_, event| {
            if event.keyval() == gtk::gdk::keys::constants::Escape {
                with(&w, |w| {
                    w.update_toolbar();
                    w.with_page(|p| p.widget().grab_focus());
                });
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });

        let w = self.weak();
        self.entry.connect_changed(move |entry| {
            with(&w, |w| {
                if entry.has_focus() {
                    w.fill_suggestions(&entry.text());
                }
            })
        });
        let w = self.weak();
        if let Some(completion) = self.entry.completion() {
            completion.connect_match_selected(move |_, model, iter| {
                let url: String = model.value(iter, 1).get().unwrap_or_default();
                with(&w, |w| {
                    w.entry.set_text(&url);
                    w.load_in_current(&url);
                });
                glib::Propagation::Stop
            });
        }
        let w = self.weak();
        self.entry.connect_icon_press(move |_, position, _| {
            with(&w, |w| match position {
                gtk::EntryIconPosition::Primary => w.toggle_blocker_for_site(),
                _ => w.toggle_bookmark(),
            })
        });
        let w = self.weak();
        self.password_button.connect_clicked(move |button| {
            with(&w, |w| {
                let uri = w.current_tab().map(|t| t.uri()).unwrap_or_default();
                crate::passwords::on_button_clicked(button, &uri);
            })
        });

        let w = self.weak();
        self.notebook.connect_switch_page(move |_, child, _| {
            with(&w, |w| {
                if let Some(tab) = w.tab_by_widget(child) {
                    tab.last_active.set(Instant::now());
                    if tab.is_sleeping() {
                        w.wake(&tab);
                    }
                    let w2 = Rc::downgrade(w);
                    glib::idle_add_local_once(move || with(&w2, |w| w.update_toolbar()));
                }
            })
        });
        let w = self.weak();
        self.notebook.connect_page_removed(move |nb, _, _| {
            if nb.n_pages() == 0 {
                with(&w, |w| w.window.close());
            }
        });

        let w = self.weak();
        self.find_entry.connect_search_changed(move |e| with(&w, |w| w.with_page(|p| p.find(&e.text(), false))));
        let w = self.weak();
        self.find_entry.connect_activate(move |e| with(&w, |w| w.with_page(|p| p.find(&e.text(), false))));
        let w = self.weak();
        self.find_entry.connect_stop_search(move |_| with(&w, |w| w.hide_find()));
        let w = self.weak();
        self.find_entry.connect_key_press_event(move |e, event| {
            let shift = event.state().contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let enter = event.keyval() == gtk::gdk::keys::constants::Return;
            if shift && enter {
                with(&w, |w| w.with_page(|p| p.find(&e.text(), true)));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let w = self.weak();
        find_prev.connect_clicked(move |_| with(&w, |w| w.with_page(|p| p.find(&w.find_entry.text(), true))));
        let w = self.weak();
        find_next.connect_clicked(move |_| with(&w, |w| w.with_page(|p| p.find(&w.find_entry.text(), false))));
        let w = self.weak();
        find_close.connect_clicked(move |_| with(&w, |w| w.hide_find()));

        let w = self.weak();
        self.window.connect_window_state_event(move |_, event| {
            let full = event.new_window_state().contains(gtk::gdk::WindowState::FULLSCREEN);
            with(&w, |w| {
                w.fullscreen.set(full);
                w.notebook.set_show_tabs(!full);
            });
            glib::Propagation::Proceed
        });

        let w = self.weak();
        self.window.connect_delete_event(move |_, _| {
            with(&w, |w| w.app.window_closing(w));
            glib::Propagation::Proceed
        });
    }

    fn add_actions(self: &Rc<Self>) {
        let actions: &[(&str, fn(&Rc<BrowserWindow>))] = &[
            ("new-tab", |w| {
                let home = w.app.config.borrow().homepage.clone();
                w.open_tab(&home, false);
                w.focus_location();
            }),
            ("close-tab", |w| {
                if let Some(tab) = w.current_tab() {
                    w.close_tab(&tab);
                }
            }),
            ("reopen-tab", |w| w.reopen_closed_tab()),
            ("new-window", |w| {
                w.app.new_window(false, true);
            }),
            ("new-private-window", |w| {
                w.app.new_window(true, true);
            }),
            ("focus-location", |w| w.focus_location()),
            ("reload", |w| w.with_page(|p| p.reload(false))),
            ("reload-hard", |w| w.with_page(|p| p.reload(true))),
            ("back", |w| w.with_page(|p| p.go_back())),
            ("forward", |w| w.with_page(|p| p.go_forward())),
            ("next-tab", |w| w.cycle_tab(1)),
            ("prev-tab", |w| w.cycle_tab(-1)),
            ("find", |w| w.show_find()),
            ("zoom-in", |w| w.zoom_by(0.1)),
            ("zoom-out", |w| w.zoom_by(-0.1)),
            ("zoom-reset", |w| {
                let z = w.app.config.borrow().default_zoom;
                w.with_page(|p| p.set_zoom(z));
            }),
            ("print", |w| w.with_page(|p| p.print(w.window.upcast_ref()))),
            ("inspector", |w| w.with_page(|p| p.show_inspector())),
            ("fullscreen", |w| {
                if w.fullscreen.get() {
                    w.window.unfullscreen();
                } else {
                    w.window.fullscreen();
                }
            }),
            ("sleep-tab", |w| {
                for tab in w.sleep_candidates() {
                    w.put_to_sleep(&tab);
                }
            }),
            ("switch-engine", |w| w.switch_engine()),
            ("downloads", |w| w.downloads_button.set_active(true)),
            ("bookmark", |w| w.toggle_bookmark()),
            ("history", |w| {
                w.open_tab("poezi://history", false);
            }),
            ("bookmarks", |w| {
                w.open_tab("poezi://bookmarks", false);
            }),
            ("settings", |w| {
                w.open_tab("poezi://settings", false);
            }),
            ("about", |w| {
                w.open_tab("poezi://about", false);
            }),
        ];
        for (name, f) in actions {
            let action = gio::SimpleAction::new(name, None);
            let w = self.weak();
            let f = *f;
            action.connect_activate(move |_, _| with(&w, f));
            self.window.add_action(&action);
        }

        let select = gio::SimpleAction::new("select-tab", Some(glib::VariantTy::INT32));
        let w = self.weak();
        select.connect_activate(move |_, param| {
            let n = param.and_then(|p| p.get::<i32>()).unwrap_or(1);
            with(&w, |w| {
                let pages = w.notebook.n_pages() as i32;
                let index = if n >= 9 { pages - 1 } else { (n - 1).min(pages - 1) };
                w.notebook.set_current_page(Some(index as u32));
            })
        });
        self.window.add_action(&select);
    }

    fn new_tab_shell(self: &Rc<Self>, background: bool) -> Rc<Tab> {
        let tab = Rc::new(Tab::new(self.app.next_tab_id()));
        let position = self.notebook.current_page().map(|p| p + 1);
        self.notebook.insert_page(&tab.container, Some(&tab.label), position);
        self.notebook.set_tab_reorderable(&tab.container, true);
        self.notebook.set_tab_detachable(&tab.container, false);

        let w = self.weak();
        let t = Rc::downgrade(&tab);
        tab.close_button.connect_clicked(move |_| {
            if let (Some(w), Some(t)) = (w.upgrade(), t.upgrade()) {
                w.close_tab(&t);
            }
        });
        let w = self.weak();
        let t = Rc::downgrade(&tab);
        tab.label.connect_button_press_event(move |_, event| {
            if event.button() == 2 {
                if let (Some(w), Some(t)) = (w.upgrade(), t.upgrade()) {
                    w.close_tab(&t);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let t = Rc::downgrade(&tab);
        tab.audio_button().connect_clicked(move |_| {
            if let Some(t) = t.upgrade() {
                if let Some(p) = t.page.borrow().as_ref() {
                    p.set_muted(!p.is_muted());
                }
                t.refresh_label();
            }
        });

        self.tabs.borrow_mut().push(tab.clone());
        if !background {
            if let Some(n) = self.notebook.page_num(&tab.container) {
                self.notebook.set_current_page(Some(n));
            }
        }
        tab
    }

    fn connect_page(self: &Rc<Self>, tab: &Rc<Tab>, page: &dyn Page) {
        let w = self.weak();
        let t = Rc::downgrade(tab);
        page.set_event_sink(Rc::new(move |event| {
            if let (Some(w), Some(t)) = (w.upgrade(), t.upgrade()) {
                w.on_page_event(&t, event);
            }
        }));
        let z = self.app.config.borrow().default_zoom;
        page.set_zoom(z);
    }

    pub fn open_tab(self: &Rc<Self>, uri: &str, background: bool) -> Rc<Tab> {
        let engine = self.app.config.borrow().engine;
        self.open_tab_with(uri, background, engine)
    }

    pub fn open_tab_with(self: &Rc<Self>, uri: &str, background: bool, engine: EngineKind) -> Rc<Tab> {
        let tab = self.new_tab_shell(background);
        let page = self.app.create_page(engine, self.private);
        self.connect_page(&tab, page.as_ref());
        page.load_uri(uri);
        tab.attach(page);
        self.update_toolbar();
        tab
    }

    pub fn add_sleeping_tab(self: &Rc<Self>, snapshot: Snapshot) -> Rc<Tab> {
        let tab = self.new_tab_shell(true);
        *tab.snapshot.borrow_mut() = Some(snapshot);
        tab.label.style_context().add_class("sleeping");
        tab.refresh_label();
        self.show_placeholder(&tab);
        tab
    }

    pub fn tabs(&self) -> Vec<Rc<Tab>> {
        let tabs = self.tabs.borrow();
        let mut ordered: Vec<Rc<Tab>> = tabs.clone();
        ordered.sort_by_key(|t| self.notebook.page_num(&t.container).unwrap_or(u32::MAX));
        ordered
    }

    pub fn current_tab(&self) -> Option<Rc<Tab>> {
        let n = self.notebook.current_page()?;
        let widget = self.notebook.nth_page(Some(n))?;
        self.tab_by_widget(&widget)
    }

    fn tab_by_widget(&self, widget: &gtk::Widget) -> Option<Rc<Tab>> {
        self.tabs.borrow().iter().find(|t| t.container.upcast_ref::<gtk::Widget>() == widget).cloned()
    }

    pub fn close_tab(self: &Rc<Self>, tab: &Rc<Tab>) {
        let uri = tab.uri();
        if !self.private && !uri.is_empty() {
            let snapshot = match tab.page.borrow().as_ref() {
                Some(p) => Snapshot { uri, title: tab.title(), engine: p.engine(), state: p.save_state() },
                None => tab.snapshot.borrow().clone().unwrap_or(Snapshot { uri, title: String::new(), engine: EngineKind::Webkit, state: None }),
            };
            let mut closed = self.closed.borrow_mut();
            closed.push(snapshot);
            if closed.len() > 25 {
                closed.remove(0);
            }
        }
        if let Some(page) = tab.page.borrow_mut().take() {
            page.close();
        }
        self.tabs.borrow_mut().retain(|t| t.id != tab.id);
        if let Some(n) = self.notebook.page_num(&tab.container) {
            self.notebook.remove_page(Some(n));
        }
        self.app.session_changed();
    }

    fn reopen_closed_tab(self: &Rc<Self>) {
        let Some(snapshot) = self.closed.borrow_mut().pop() else { return };
        let tab = self.add_sleeping_tab(snapshot);
        if let Some(n) = self.notebook.page_num(&tab.container) {
            self.notebook.set_current_page(Some(n));
        }
    }

    pub fn wake(self: &Rc<Self>, tab: &Rc<Tab>) {
        let Some(snapshot) = tab.snapshot.borrow_mut().take() else { return };
        for child in tab.container.children() {
            tab.container.remove(&child);
        }
        let engine = if snapshot.engine.available() { snapshot.engine } else { EngineKind::Webkit };
        let page = self.app.create_page(engine, self.private);
        self.connect_page(tab, page.as_ref());
        let restored = snapshot.state.as_deref().is_some_and(|state| page.restore_state(state));
        if !restored {
            page.load_uri(&snapshot.uri);
        }
        tab.attach(page);
        tab.last_active.set(Instant::now());
    }

    fn show_placeholder(self: &Rc<Self>, tab: &Rc<Tab>) {
        for child in tab.container.children() {
            tab.container.remove(&child);
        }
        let label = gtk::Label::new(Some("Dieser Tab schläft, um Arbeitsspeicher zu sparen."));
        label.style_context().add_class("dim-label");
        let button = gtk::Button::with_label("Aufwecken");
        button.set_halign(Align::Center);
        let w = self.weak();
        let t = Rc::downgrade(tab);
        button.connect_clicked(move |_| {
            if let (Some(w), Some(t)) = (w.upgrade(), t.upgrade()) {
                w.wake(&t);
                w.update_toolbar();
            }
        });
        let bx = gtk::Box::new(Orientation::Vertical, 12);
        bx.set_valign(Align::Center);
        bx.set_vexpand(true);
        bx.pack_start(&label, false, false, 0);
        bx.pack_start(&button, false, false, 0);
        bx.show_all();
        tab.container.pack_start(&bx, true, true, 0);
    }

    pub fn sleep_candidates(&self) -> Vec<Rc<Tab>> {
        let current = self.current_tab().map(|t| t.id);
        self.tabs
            .borrow()
            .iter()
            .filter(|t| Some(t.id) != current && !t.is_sleeping())
            .filter(|t| !t.page.borrow().as_ref().is_some_and(|p| p.is_playing_audio()))
            .cloned()
            .collect()
    }

    pub fn put_to_sleep(self: &Rc<Self>, tab: &Rc<Tab>) {
        if tab.sleep() {
            self.show_placeholder(tab);
        }
    }

    pub fn sleep_idle_tabs(self: &Rc<Self>, minutes: u32) {
        let limit = std::time::Duration::from_secs(minutes as u64 * 60);
        for tab in self.sleep_candidates() {
            if tab.last_active.get().elapsed() >= limit {
                self.put_to_sleep(&tab);
            }
        }
    }

    fn cycle_tab(&self, delta: i32) {
        let pages = self.notebook.n_pages() as i32;
        if pages == 0 {
            return;
        }
        let current = self.notebook.current_page().unwrap_or(0) as i32;
        self.notebook.set_current_page(Some(((current + delta).rem_euclid(pages)) as u32));
    }

    fn switch_engine(self: &Rc<Self>) {
        let Some(tab) = self.current_tab() else { return };
        let Some(current) = tab.engine() else { return };
        let other = match current {
            EngineKind::Webkit => EngineKind::Servo,
            EngineKind::Servo => EngineKind::Webkit,
        };
        if !other.available() {
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Info,
                gtk::ButtonsType::Ok,
                "Diese Version wurde ohne Servo gebaut.",
            );
            dialog.set_secondary_text(Some("Baue PoeziBrowse mit „cargo build --release --features servo“, um die Servo-Engine zu nutzen."));
            dialog.connect_response(|d, _| d.close());
            dialog.show_all();
            return;
        }
        let uri = tab.uri();
        let new_tab = self.open_tab_with(&uri, false, other);
        self.close_tab(&tab);
        new_tab.refresh_label();
    }

    fn on_page_event(self: &Rc<Self>, tab: &Rc<Tab>, event: PageEvent) {
        let is_current = self.current_tab().is_some_and(|t| t.id == tab.id);
        match event {
            PageEvent::TitleChanged | PageEvent::FaviconChanged | PageEvent::AudioChanged => {
                tab.refresh_label();
                if is_current {
                    self.update_title();
                }
            }
            PageEvent::LoadingChanged(_) => {
                tab.refresh_label();
                if is_current {
                    self.update_toolbar();
                }
            }
            PageEvent::UriChanged | PageEvent::NavigationStateChanged => {
                if is_current {
                    self.update_toolbar();
                }
            }
            PageEvent::Progress(p) => {
                if is_current {
                    self.entry.set_progress_fraction(if p >= 1.0 { 0.0 } else { p });
                }
            }
            PageEvent::LoadFinished => {
                if is_current {
                    self.entry.set_progress_fraction(0.0);
                }
                if !self.private {
                    if let Some(page) = tab.page.borrow().as_ref() {
                        self.app.page_visited(&page.uri().unwrap_or_default(), &page.title().unwrap_or_default());
                    }
                }
                self.app.session_changed();
            }
            PageEvent::HoverLink(link) => match link {
                Some(link) if is_current => {
                    self.status.set_text(&link);
                    self.status.show();
                }
                _ => self.status.hide(),
            },
            PageEvent::OpenInNewTab(uri, background) => {
                let engine = tab.engine().unwrap_or_default();
                self.open_tab_with(&uri, background, engine);
            }
            PageEvent::Popup(page) => {
                let new_tab = self.new_tab_shell(false);
                self.connect_page(&new_tab, page.as_ref());
                new_tab.attach(page);
                self.update_toolbar();
            }
            PageEvent::Fullscreen(full) => {
                if full {
                    self.window.fullscreen();
                } else {
                    self.window.unfullscreen();
                }
            }
            PageEvent::Crashed => {
                if let Some(page) = tab.page.borrow_mut().take() {
                    *tab.snapshot.borrow_mut() = Some(Snapshot {
                        uri: page.uri().unwrap_or_default(),
                        title: page.title().unwrap_or_default(),
                        engine: page.engine(),
                        state: None,
                    });
                }
                tab.label.style_context().add_class("sleeping");
                self.show_placeholder(tab);
                tab.refresh_label();
            }
            PageEvent::CloseRequested => self.close_tab(tab),
        }
    }

    fn with_page(&self, f: impl FnOnce(&dyn Page)) {
        if let Some(tab) = self.current_tab() {
            if let Some(page) = tab.page.borrow().as_ref() {
                f(page.as_ref());
            }
        }
    }

    fn load_in_current(self: &Rc<Self>, uri: &str) {
        match self.current_tab() {
            Some(tab) => {
                if tab.is_sleeping() {
                    self.wake(&tab);
                }
                if let Some(page) = tab.page.borrow().as_ref() {
                    page.load_uri(uri);
                    page.widget().grab_focus();
                }
            }
            None => {
                self.open_tab(uri, false);
            }
        }
    }

    pub fn update_toolbar(&self) {
        let tab = self.current_tab();
        let page_ref = tab.as_ref().map(|t| t.page.borrow());
        let page = page_ref.as_ref().and_then(|p| p.as_ref());
        self.back.set_sensitive(page.is_some_and(|p| p.can_go_back()));
        self.forward.set_sensitive(page.is_some_and(|p| p.can_go_forward()));
        let loading = page.is_some_and(|p| p.is_loading());
        self.reload.set_image(Some(&gtk::Image::from_icon_name(
            Some(if loading { "process-stop-symbolic" } else { "view-refresh-symbolic" }),
            gtk::IconSize::Button,
        )));
        self.reload.set_tooltip_text(Some(if loading { "Laden abbrechen" } else { "Neu laden (Strg+R)" }));
        if !loading {
            self.entry.set_progress_fraction(0.0);
        }
        let uri = tab.as_ref().map(|t| t.uri()).unwrap_or_default();
        self.update_entry_icons(&uri);
        if !self.entry.has_focus() {
            let shown = if uri.starts_with("poezi://start") { "" } else { uri.as_str() };
            self.entry.set_text(shown);
        }
        drop(page_ref);
        self.update_title();
    }

    fn update_entry_icons(&self, uri: &str) {
        let web = uri.starts_with("http://") || uri.starts_with("https://");
        let (star, star_tip) = if !web {
            (None, None)
        } else if self.app.storage.is_bookmarked(uri) {
            (Some("starred-symbolic"), Some("Lesezeichen entfernen (Strg+D)"))
        } else {
            (Some("non-starred-symbolic"), Some("Lesezeichen hinzufügen (Strg+D)"))
        };
        self.entry.set_icon_from_icon_name(gtk::EntryIconPosition::Secondary, star);
        self.entry.set_icon_tooltip_text(gtk::EntryIconPosition::Secondary, star_tip);

        let host = config::host_of(uri).unwrap_or_default();
        let blocker_on = self.app.blocker_rules().is_some();
        let (shield, shield_tip) = if !web || !blocker_on {
            (None, None)
        } else if self.app.config.borrow().is_allowlisted(&host) {
            (Some("security-low-symbolic"), Some(format!("Blocker ist auf {host} aus – klicken zum Einschalten")))
        } else {
            (Some("security-high-symbolic"), Some(format!("Werbung und Tracker werden blockiert – klicken, um den Blocker auf {host} auszuschalten")))
        };
        self.entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, shield);
        self.entry.set_icon_tooltip_text(gtk::EntryIconPosition::Primary, shield_tip.as_deref());
    }

    fn toggle_bookmark(&self) {
        let Some(tab) = self.current_tab() else { return };
        let uri = tab.uri();
        if !(uri.starts_with("http://") || uri.starts_with("https://")) {
            return;
        }
        if self.app.storage.is_bookmarked(&uri) {
            self.app.storage.remove_bookmark(&uri);
        } else {
            self.app.storage.add_bookmark(&uri, &tab.title());
        }
        self.update_entry_icons(&uri);
    }

    fn toggle_blocker_for_site(&self) {
        let Some(tab) = self.current_tab() else { return };
        let uri = tab.uri();
        let Some(host) = config::host_of(&uri) else { return };
        let host = host.trim_start_matches("www.").to_string();
        {
            let mut config = self.app.config.borrow_mut();
            if config.is_allowlisted(&host) {
                config.block_allowlist.retain(|h| !(host == *h || host.ends_with(&format!(".{h}"))));
            } else {
                config.block_allowlist.push(host);
            }
            config.save();
        }
        self.update_entry_icons(&uri);
        self.with_page(|p| p.reload(false));
    }

    fn fill_suggestions(&self, text: &str) {
        self.suggestions.clear();
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        for e in self.app.storage.suggest(text, 8) {
            let title = if e.title.is_empty() { e.url.clone() } else { e.title.clone() };
            let markup = format!(
                "{}  <span alpha=\"60%\" size=\"small\">{}</span>",
                glib::markup_escape_text(&title),
                glib::markup_escape_text(&e.url)
            );
            self.suggestions.insert_with_values(None, &[(0, &markup), (1, &e.url)]);
        }
    }

    fn update_title(&self) {
        let title = self.current_tab().map(|t| t.title()).unwrap_or_default();
        let suffix = if self.private { "PoeziBrowse (privat)" } else { "PoeziBrowse" };
        self.window.set_title(&if title.is_empty() { suffix.to_string() } else { format!("{title} – {suffix}") });
    }

    fn focus_location(&self) {
        self.entry.grab_focus();
    }

    fn zoom_by(&self, delta: f64) {
        self.with_page(|p| p.set_zoom((p.zoom() + delta).clamp(0.3, 5.0)));
    }

    fn show_find(&self) {
        self.find_revealer.set_reveal_child(true);
        self.find_entry.grab_focus();
    }

    fn hide_find(&self) {
        self.find_revealer.set_reveal_child(false);
        self.with_page(|p| {
            p.find("", false);
            p.widget().grab_focus();
        });
    }

    pub fn select_page(&self, n: u32) {
        self.notebook.set_current_page(Some(n));
    }

    pub fn show_downloads(&self) {
        self.downloads_button.set_active(true);
    }
}

fn with<T>(weak: &Weak<T>, f: impl FnOnce(&Rc<T>)) {
    if let Some(strong) = weak.upgrade() {
        f(&strong);
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(Some(icon), gtk::IconSize::Button);
    b.set_tooltip_text(Some(tooltip));
    b.set_valign(Align::Center);
    b
}

fn main_menu(private: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    let windows = gio::Menu::new();
    windows.append(Some("Neuer Tab"), Some("win.new-tab"));
    windows.append(Some("Neues Fenster"), Some("win.new-window"));
    windows.append(Some("Neues privates Fenster"), Some("win.new-private-window"));
    menu.append_section(None, &windows);

    let page = gio::Menu::new();
    page.append(Some("Auf der Seite suchen …"), Some("win.find"));
    page.append(Some("Drucken …"), Some("win.print"));
    page.append(Some("Vollbild"), Some("win.fullscreen"));
    let zoom = gio::Menu::new();
    zoom.append(Some("Vergrößern"), Some("win.zoom-in"));
    zoom.append(Some("Verkleinern"), Some("win.zoom-out"));
    zoom.append(Some("Originalgröße"), Some("win.zoom-reset"));
    page.append_submenu(Some("Zoom"), &zoom);
    menu.append_section(None, &page);

    let ram = gio::Menu::new();
    ram.append(Some("Hintergrund-Tabs schlafen legen"), Some("win.sleep-tab"));
    ram.append(Some("Engine wechseln (WebKit ↔ Servo)"), Some("win.switch-engine"));
    menu.append_section(None, &ram);

    let misc = gio::Menu::new();
    misc.append(Some("Lesezeichen"), Some("win.bookmarks"));
    misc.append(Some("Verlauf"), Some("win.history"));
    misc.append(Some("Downloads"), Some("win.downloads"));
    if !private {
        misc.append(Some("Einstellungen"), Some("win.settings"));
    }
    misc.append(Some("Entwicklerwerkzeuge"), Some("win.inspector"));
    misc.append(Some("Über PoeziBrowse"), Some("win.about"));
    menu.append_section(None, &misc);
    menu
}
