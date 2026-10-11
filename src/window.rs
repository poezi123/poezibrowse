use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Align, Orientation};

use crate::app::App;
use crate::config::{self, EngineKind, Mode};
use crate::engine::{Page, PageEvent};
use crate::sidebar::{self, Icon};
use crate::tab::{Snapshot, Tab};
use crate::titlebar::TitleBar;

const TAB_TARGET: &str = "application/x-poezibrowse-tab";

pub struct BrowserWindow {
    app: Rc<App>,
    pub window: gtk::ApplicationWindow,
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
    password_button: gtk::Button,
    suggestions: gtk::ListStore,
    favorites_bar: gtk::Box,
    favorites: gtk::Box,
    strip: gtk::Box,
    tabs_box: gtk::Box,
    strip_scroll: gtk::ScrolledWindow,
    sidebar: gtk::Box,
    panel_holder: gtk::Box,
    panel_stack: gtk::Stack,
    panel_toggles: Vec<(String, gtk::ToggleButton)>,
    bookmarks_panel: sidebar::Panel,
    history_panel: sidebar::Panel,
    mode_badge: gtk::Label,
    tabs: RefCell<Vec<Rc<Tab>>>,
    closed: RefCell<Vec<Snapshot>>,
    fullscreen: Cell<bool>,
    dragged_tab: Cell<Option<u64>>,
    syncing_toggles: Cell<bool>,
}

impl BrowserWindow {
    pub fn new(app: &Rc<App>) -> Rc<BrowserWindow> {
        let window = gtk::ApplicationWindow::new(&app.gtk_app);
        window.set_default_size(1280, 820);
        window.set_icon_name(Some(config::APP_ID));
        let scale = window.scale_factor().max(1);

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
        let mode_badge = gtk::Label::new(None);
        mode_badge.style_context().add_class("private-badge");
        mode_badge.set_valign(Align::Center);
        mode_badge.set_no_show_all(true);
        header.pack_start(&mode_badge);

        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_width_chars(40);
        entry.set_max_width_chars(110);
        entry.set_placeholder_text(Some("Suchen oder Adresse eingeben"));
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
        menu_button.set_menu_model(Some(&main_menu()));
        menu_button.set_tooltip_text(Some("Menü"));
        header.pack_end(&menu_button);
        let downloads_button = icon_button("folder-download-symbolic", "Downloads (Strg+Umschalt+Y)");
        downloads_button.set_action_name(Some("win.downloads"));
        header.pack_end(&downloads_button);
        let password_button = icon_button("dialog-password-symbolic", "Passwörter");
        header.pack_end(&password_button);

        window.set_titlebar(Some(&header));
        let titlebar = TitleBar::install(&header, &app.config.borrow(), Mode::Normal);
        window.style_context().add_class("poezi");

        let favorites = gtk::Box::new(Orientation::Horizontal, 2);
        let favorites_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .build();
        favorites_scroll.add(&favorites);
        let favorites_bar = gtk::Box::new(Orientation::Horizontal, 0);
        favorites_bar.style_context().add_class("favorites-bar");
        favorites_bar.pack_start(&favorites_scroll, true, true, 0);

        let tabs_box = gtk::Box::new(Orientation::Horizontal, 2);
        let new_tab = icon_button("list-add-symbolic", "Neuer Tab (Strg+T)");
        new_tab.set_relief(gtk::ReliefStyle::None);
        new_tab.set_action_name(Some("win.new-tab"));
        new_tab.style_context().add_class("new-tab");
        let strip_content = gtk::Box::new(Orientation::Horizontal, 2);
        strip_content.pack_start(&tabs_box, false, false, 0);
        strip_content.pack_start(&new_tab, false, false, 0);
        let strip_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .build();
        strip_scroll.add(&strip_content);
        let ram_label = gtk::Label::new(None);
        ram_label.style_context().add_class("ram-label");
        ram_label.set_tooltip_text(Some("Arbeitsspeicher von PoeziBrowse insgesamt (alle Prozesse)"));
        let strip = gtk::Box::new(Orientation::Horizontal, 6);
        strip.style_context().add_class("tab-strip");
        strip.pack_start(&strip_scroll, true, true, 0);
        strip.pack_end(&ram_label, false, false, 0);

        let notebook = gtk::Notebook::new();
        notebook.set_show_tabs(false);
        notebook.set_show_border(false);
        notebook.style_context().add_class("poezi");

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

        let bookmarks_panel = sidebar::panel("Lesezeichen", true, "Noch keine Lesezeichen.\nKlicke auf den Stern in der Adressleiste.");
        let history_panel = sidebar::panel("Verlauf", true, "Der Verlauf ist leer.");
        let clear_history = gtk::Button::with_label("Verlauf löschen");
        clear_history.style_context().add_class("destructive-action");
        history_panel.footer.pack_end(&clear_history, false, false, 0);
        let downloads_panel = sidebar::panel("Downloads", false, "Noch keine Downloads.");
        let downloads = downloads_panel.list.clone();
        let open_folder = gtk::Button::with_label("Download-Ordner öffnen");
        downloads_panel.footer.pack_end(&open_folder, false, false, 0);
        let panel_stack = gtk::Stack::new();
        panel_stack.add_named(&bookmarks_panel.root, "bookmarks");
        panel_stack.add_named(&history_panel.root, "history");
        panel_stack.add_named(&downloads_panel.root, "downloads");
        panel_stack.set_hhomogeneous(false);
        let panel_holder = gtk::Box::new(Orientation::Vertical, 0);
        panel_holder.set_size_request(300, -1);
        panel_holder.set_hexpand(false);
        panel_holder.pack_start(&panel_stack, true, true, 0);
        panel_holder.set_no_show_all(true);
        panel_stack.show_all();

        let sidebar = gtk::Box::new(Orientation::Vertical, 4);
        sidebar.style_context().add_class("sidebar");
        let mut panel_toggles = Vec::new();
        for (name, icon, tip) in [
            ("bookmarks", Icon::Theme("user-bookmarks-symbolic"), "Lesezeichen (Strg+Umschalt+O)"),
            ("history", Icon::Theme("document-open-recent-symbolic"), "Verlauf (Strg+H)"),
            ("downloads", Icon::Theme("folder-download-symbolic"), "Downloads (Strg+Umschalt+Y)"),
        ] {
            let toggle = sidebar::toggle(&icon, tip, scale);
            sidebar.pack_start(&toggle, false, false, 0);
            panel_toggles.push((name.to_string(), toggle));
        }
        sidebar.pack_start(&gtk::Separator::new(Orientation::Horizontal), false, false, 4);
        for (action, icon, tip) in [
            ("win.new-private-tab", sidebar::PRIVATE_ICON, "Neuer privater Tab (Strg+Umschalt+P)"),
            ("win.new-tor-tab", sidebar::TOR_ICON, "Neuer Tor-Tab (Strg+Umschalt+U)"),
            ("win.sleep-tab", Icon::Theme("weather-clear-night-symbolic"), "Hintergrund-Tabs schlafen legen (RAM freigeben)"),
        ] {
            let b = sidebar::button(&icon, tip, scale);
            b.set_action_name(Some(action));
            sidebar.pack_start(&b, false, false, 0);
        }
        for (action, icon, tip) in [
            ("win.about", Icon::Theme("help-about-symbolic"), "Über PoeziBrowse und RAM-Verbrauch"),
            ("win.settings", Icon::Theme("emblem-system-symbolic"), "Einstellungen"),
        ] {
            let b = sidebar::button(&icon, tip, scale);
            b.set_action_name(Some(action));
            sidebar.pack_end(&b, false, false, 0);
        }

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

        let content = gtk::Box::new(Orientation::Vertical, 0);
        content.pack_start(&overlay, true, true, 0);
        content.pack_start(&find_revealer, false, false, 0);
        let body = gtk::Box::new(Orientation::Horizontal, 0);
        body.pack_start(&sidebar, false, false, 0);
        body.pack_start(&panel_holder, false, false, 0);
        body.pack_start(&content, true, true, 0);
        let root = gtk::Box::new(Orientation::Vertical, 0);
        root.pack_start(&favorites_bar, false, false, 0);
        root.pack_start(&strip, false, false, 0);
        root.pack_start(&body, true, true, 0);
        window.add(&root);

        let this = Rc::new(BrowserWindow {
            app: app.clone(),
            window,
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
            password_button,
            suggestions,
            favorites_bar,
            favorites,
            strip,
            tabs_box,
            strip_scroll,
            sidebar,
            panel_holder,
            panel_stack,
            panel_toggles,
            bookmarks_panel,
            history_panel,
            mode_badge,
            tabs: RefCell::new(Vec::new()),
            closed: RefCell::new(Vec::new()),
            fullscreen: Cell::new(false),
            dragged_tab: Cell::new(None),
            syncing_toggles: Cell::new(false),
        });
        this.connect_ui(&find_prev, &find_next, &find_close);
        this.connect_panels(&clear_history, &open_folder);
        this.add_actions();
        this.window.show_all();
        this.rebuild_favorites();
        this.layout_changed();
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
            with(&w, |w| w.with_page(|p| if p.is_loading() { p.stop() } else { p.reload(false) }))
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
                    for t in w.tabs.borrow().iter() {
                        t.set_active(t.id == tab.id);
                    }
                    let w2 = Rc::downgrade(w);
                    let t2 = Rc::downgrade(&tab);
                    glib::idle_add_local_once(move || {
                        with(&w2, |w| {
                            w.update_toolbar();
                            if let Some(t) = t2.upgrade() {
                                w.scroll_to_tab(&t);
                            }
                        })
                    });
                }
            })
        });

        let w = self.weak();
        self.strip_scroll.connect_scroll_event(move |scroll, event| {
            let delta = match event.direction() {
                gtk::gdk::ScrollDirection::Up | gtk::gdk::ScrollDirection::Left => -1.0,
                gtk::gdk::ScrollDirection::Down | gtk::gdk::ScrollDirection::Right => 1.0,
                gtk::gdk::ScrollDirection::Smooth => {
                    let (dx, dy) = event.delta();
                    if dx.abs() > dy.abs() { dx } else { dy }
                }
                _ => 0.0,
            };
            let adj = scroll.hadjustment();
            if adj.upper() - adj.page_size() > 1.0 {
                adj.set_value(adj.value() + delta * 60.0);
            } else if delta.abs() >= 1.0 {
                with(&w, |w| w.cycle_tab(if delta > 0.0 { 1 } else { -1 }));
            }
            glib::Propagation::Stop
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
                w.layout_changed();
            });
            glib::Propagation::Proceed
        });

        let w = self.weak();
        self.window.connect_delete_event(move |_, _| {
            with(&w, |w| w.app.window_closing(w));
            glib::Propagation::Proceed
        });
    }

    fn connect_panels(self: &Rc<Self>, clear_history: &gtk::Button, open_folder: &gtk::Button) {
        for (name, toggle) in &self.panel_toggles {
            let w = self.weak();
            let name = name.clone();
            toggle.connect_toggled(move |toggle| {
                with(&w, |w| {
                    if w.syncing_toggles.get() {
                        return;
                    }
                    if toggle.is_active() {
                        w.show_panel(&name);
                    } else {
                        w.hide_panel();
                    }
                })
            });
        }
        if let Some(search) = &self.bookmarks_panel.search {
            let w = self.weak();
            search.connect_search_changed(move |_| with(&w, |w| w.refresh_bookmarks_panel()));
        }
        if let Some(search) = &self.history_panel.search {
            let w = self.weak();
            search.connect_search_changed(move |_| with(&w, |w| w.refresh_history_panel()));
        }
        let w = self.weak();
        clear_history.connect_clicked(move |_| {
            with(&w, |w| {
                w.app.storage.clear_history();
                w.app.history_changed();
            })
        });
        let w = self.weak();
        open_folder.connect_clicked(move |_| {
            with(&w, |w| {
                let dir = w.app.config.borrow().download_dir();
                let uri = glib::filename_to_uri(&dir, None).map(|u| u.to_string()).unwrap_or_default();
                let _ = gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>);
            })
        });
    }

    fn add_actions(self: &Rc<Self>) {
        let actions: &[(&str, fn(&Rc<BrowserWindow>))] = &[
            ("new-tab", |w| {
                w.open_tab(&w.app.homepage(Mode::Normal), false);
                w.focus_location();
            }),
            ("new-private-tab", |w| {
                w.open_mode_tab(Mode::Private);
            }),
            ("new-tor-tab", |w| {
                w.open_mode_tab(Mode::Tor);
            }),
            ("close-tab", |w| {
                if let Some(tab) = w.current_tab() {
                    w.close_tab(&tab);
                }
            }),
            ("reopen-tab", |w| w.reopen_closed_tab()),
            ("new-window", |w| {
                w.app.new_window(true);
            }),
            ("focus-location", |w| w.focus_location()),
            ("reload", |w| w.with_page(|p| p.reload(false))),
            ("reload-hard", |w| w.with_page(|p| p.reload(true))),
            ("back", |w| w.with_page(|p| p.go_back())),
            ("forward", |w| w.with_page(|p| p.go_forward())),
            ("next-tab", |w| w.cycle_tab(1)),
            ("prev-tab", |w| w.cycle_tab(-1)),
            ("move-tab-left", |w| w.move_current_tab(-1)),
            ("move-tab-right", |w| w.move_current_tab(1)),
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
                for engine in w.app.engines() {
                    engine.release_memory();
                }
            }),
            ("switch-engine", |w| w.switch_engine()),
            ("downloads", |w| w.toggle_panel("downloads")),
            ("bookmark", |w| w.toggle_bookmark()),
            ("history", |w| w.toggle_panel("history")),
            ("bookmarks", |w| w.toggle_panel("bookmarks")),
            ("toggle-sidebar", |w| {
                let show = !w.app.config.borrow().show_sidebar;
                w.app.set_layout_option(|c| c.show_sidebar = show);
            }),
            ("toggle-favorites", |w| {
                let show = !w.app.config.borrow().show_favorites;
                w.app.set_layout_option(|c| c.show_favorites = show);
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
                if pages == 0 {
                    return;
                }
                let index = if n >= 9 { pages - 1 } else { (n - 1).min(pages - 1) };
                w.notebook.set_current_page(Some(index as u32));
            })
        });
        self.window.add_action(&select);
    }

    fn new_tab_shell(self: &Rc<Self>, background: bool, mode: Mode) -> Rc<Tab> {
        let tab = Rc::new(Tab::new(self.app.next_tab_id(), mode));
        let position = self.notebook.current_page().map(|p| p + 1);
        let index = self.notebook.insert_page(&tab.container, None::<&gtk::Widget>, position);
        self.tabs_box.pack_start(&tab.label, false, false, 0);
        self.tabs_box.reorder_child(&tab.label, index as i32);

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
            let (Some(w), Some(t)) = (w.upgrade(), t.upgrade()) else { return glib::Propagation::Proceed };
            match event.button() {
                1 => {
                    if let Some(n) = w.notebook.page_num(&t.container) {
                        w.notebook.set_current_page(Some(n));
                    }
                    glib::Propagation::Proceed
                }
                2 => {
                    w.close_tab(&t);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
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
        self.enable_tab_drag(&tab);

        self.tabs.borrow_mut().push(tab.clone());
        if !background {
            self.notebook.set_current_page(Some(index));
        }
        tab
    }

    fn enable_tab_drag(self: &Rc<Self>, tab: &Rc<Tab>) {
        let targets = [gtk::TargetEntry::new(TAB_TARGET, gtk::TargetFlags::SAME_APP, 0)];
        tab.label.drag_source_set(gtk::gdk::ModifierType::BUTTON1_MASK, &targets, gtk::gdk::DragAction::MOVE);
        tab.label.drag_dest_set(gtk::DestDefaults::ALL, &targets, gtk::gdk::DragAction::MOVE);
        let w = self.weak();
        let id = tab.id;
        tab.label.connect_drag_begin(move |_, _| with(&w, |w| w.dragged_tab.set(Some(id))));
        tab.label.connect_drag_data_get(move |_, _, data, _, _| {
            data.set(&data.target(), 8, id.to_string().as_bytes());
        });
        let w = self.weak();
        tab.label.connect_drag_end(move |_, _| with(&w, |w| w.dragged_tab.set(None)));
        let w = self.weak();
        let t = Rc::downgrade(tab);
        tab.label.connect_drag_data_received(move |label, _, x, _, _, _, _| {
            let (Some(w), Some(target)) = (w.upgrade(), t.upgrade()) else { return };
            let Some(source_id) = w.dragged_tab.get() else { return };
            if source_id == target.id {
                return;
            }
            let Some(source) = w.tabs.borrow().iter().find(|t| t.id == source_id).cloned() else { return };
            let Some(mut index) = w.notebook.page_num(&target.container) else { return };
            if x > label.allocated_width() / 2 {
                index += 1;
            }
            let from = w.notebook.page_num(&source.container).unwrap_or(0);
            if from < index {
                index -= 1;
            }
            w.move_tab(&source, index);
        });
    }

    fn move_tab(&self, tab: &Rc<Tab>, index: u32) {
        self.notebook.reorder_child(&tab.container, Some(index));
        self.tabs_box.reorder_child(&tab.label, index as i32);
        self.app.session_changed();
    }

    fn move_current_tab(&self, delta: i32) {
        let Some(tab) = self.current_tab() else { return };
        let Some(current) = self.notebook.page_num(&tab.container) else { return };
        let pages = self.notebook.n_pages() as i32;
        let target = (current as i32 + delta).clamp(0, pages - 1) as u32;
        self.move_tab(&tab, target);
    }

    fn scroll_to_tab(&self, tab: &Rc<Tab>) {
        let alloc = tab.label.allocation();
        let adj = self.strip_scroll.hadjustment();
        adj.clamp_page(alloc.x() as f64, (alloc.x() + alloc.width()) as f64);
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
        self.open_tab_in(uri, background, Mode::Normal)
    }

    pub fn open_tab_in(self: &Rc<Self>, uri: &str, background: bool, mode: Mode) -> Rc<Tab> {
        let engine = self.app.config.borrow().engine;
        self.open_tab_with(uri, background, engine, mode)
    }

    pub fn open_mode_tab(self: &Rc<Self>, mode: Mode) -> Rc<Tab> {
        if mode == Mode::Tor {
            self.app.tor.start(&self.app.config.borrow());
        }
        let tab = self.open_tab_in(&self.app.homepage(mode), false, mode);
        self.focus_location();
        tab
    }

    pub fn has_tab_mode(&self, mode: Mode) -> bool {
        self.tabs.borrow().iter().any(|t| t.mode == mode)
    }

    pub fn current_mode(&self) -> Mode {
        self.current_tab().map(|t| t.mode).unwrap_or(Mode::Normal)
    }

    pub fn open_tab_with(self: &Rc<Self>, uri: &str, background: bool, engine: EngineKind, mode: Mode) -> Rc<Tab> {
        let tab = self.new_tab_shell(background, mode);
        let page = self.app.create_page(engine, mode);
        self.connect_page(&tab, page.as_ref());
        page.load_uri(uri);
        tab.attach(page);
        tab.set_active(self.current_tab().is_some_and(|t| t.id == tab.id));
        self.update_toolbar();
        tab
    }

    pub fn add_sleeping_tab(self: &Rc<Self>, snapshot: Snapshot) -> Rc<Tab> {
        let tab = self.new_tab_shell(true, Mode::Normal);
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
        if tab.mode == Mode::Normal && !uri.is_empty() {
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
        if self.tabs.borrow().len() == 1 {
            self.open_tab(&self.app.homepage(Mode::Normal), false);
        }
        if let Some(page) = tab.page.borrow_mut().take() {
            page.close();
        }
        self.tabs.borrow_mut().retain(|t| t.id != tab.id);
        self.tabs_box.remove(&tab.label);
        if let Some(n) = self.notebook.page_num(&tab.container) {
            self.notebook.remove_page(Some(n));
        }
        self.app.session_changed();
        if tab.mode != Mode::Normal {
            self.app.forget_unused_modes();
        }
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
        let page = self.app.create_page(engine, tab.mode);
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

    pub fn reload_tabs_starting_with(&self, prefix: &str) {
        for tab in self.tabs.borrow().iter() {
            if let Some(page) = tab.page.borrow().as_ref() {
                if page.uri().is_some_and(|u| u.starts_with(prefix)) {
                    page.reload(false);
                }
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
        let tor = tab.mode == Mode::Tor;
        if !other.available() || tor {
            let dialog = gtk::MessageDialog::new(
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                gtk::MessageType::Info,
                gtk::ButtonsType::Ok,
                if tor { "Im Tor-Modus läuft nur WebKit." } else { "Diese Version wurde ohne Servo gebaut." },
            );
            if !tor {
                dialog.set_secondary_text(Some("Baue PoeziBrowse mit „cargo build --release --features servo“, um die Servo-Engine zu nutzen."));
            }
            dialog.connect_response(|d, _| d.close());
            dialog.show_all();
            return;
        }
        let uri = tab.uri();
        let new_tab = self.open_tab_with(&uri, false, other, tab.mode);
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
            PageEvent::LoadingChanged => {
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
                if tab.mode == Mode::Normal {
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
                self.open_tab_with(&uri, background, engine, tab.mode);
            }
            PageEvent::Popup(page) => {
                let new_tab = self.new_tab_shell(false, tab.mode);
                self.connect_page(&new_tab, page.as_ref());
                new_tab.attach(page);
                new_tab.set_active(true);
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

    pub fn load_in_current(self: &Rc<Self>, uri: &str) {
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
        self.update_mode_look(tab.as_ref().map(|t| t.mode).unwrap_or(Mode::Normal));
        if !self.entry.has_focus() {
            let shown = if uri.starts_with("poezi://start") || uri.starts_with("poezi://tor") { "" } else { uri.as_str() };
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
            (Some("starred-symbolic"), Some("Aus den Favoriten entfernen (Strg+D)"))
        } else {
            (Some("non-starred-symbolic"), Some("Zu den Favoriten hinzufügen (Strg+D)"))
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
        self.app.bookmarks_changed();
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
        let suffix = match self.current_mode() {
            Mode::Normal => "PoeziBrowse",
            Mode::Private => "PoeziBrowse (privat)",
            Mode::Tor => "PoeziBrowse (Tor)",
        };
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

    pub fn show_downloads(self: &Rc<Self>) {
        self.show_panel("downloads");
    }

    fn toggle_panel(self: &Rc<Self>, name: &str) {
        let open = self.panel_holder.is_visible() && self.panel_stack.visible_child_name().as_deref() == Some(name);
        if open {
            self.hide_panel();
        } else {
            self.show_panel(name);
        }
    }

    fn show_panel(self: &Rc<Self>, name: &str) {
        match name {
            "bookmarks" => self.refresh_bookmarks_panel(),
            "history" => self.refresh_history_panel(),
            _ => {}
        }
        self.panel_stack.set_visible_child_name(name);
        self.panel_holder.show();
        self.sync_toggles(Some(name));
    }

    fn hide_panel(&self) {
        self.panel_holder.hide();
        self.sync_toggles(None);
    }

    fn sync_toggles(&self, active: Option<&str>) {
        self.syncing_toggles.set(true);
        for (name, toggle) in &self.panel_toggles {
            toggle.set_active(Some(name.as_str()) == active);
        }
        self.syncing_toggles.set(false);
    }

    fn open_from_panel(self: &Rc<Self>, url: &str, event: &gtk::gdk::EventButton) {
        let new_tab = event.button() == 2 || event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK);
        if new_tab {
            self.open_tab(url, true);
        } else {
            self.load_in_current(url);
        }
    }

    pub fn refresh_bookmarks_panel(self: &Rc<Self>) {
        let list = &self.bookmarks_panel.list;
        for child in list.children() {
            list.remove(&child);
        }
        let search = self.bookmarks_panel.search.as_ref().map(|s| s.text().to_lowercase()).unwrap_or_default();
        for e in self.app.storage.bookmarks() {
            if !search.is_empty() && !e.title.to_lowercase().contains(&search) && !e.url.to_lowercase().contains(&search) {
                continue;
            }
            let row = sidebar::entry_row(&e.title, &e.url, None);
            let w = self.weak();
            let url = e.url.clone();
            row.open.connect_button_press_event(move |_, event| {
                with(&w, |w| w.open_from_panel(&url, event));
                glib::Propagation::Stop
            });
            let w = self.weak();
            let url = e.url.clone();
            row.remove.connect_clicked(move |_| {
                with(&w, |w| {
                    w.app.storage.remove_bookmark(&url);
                    w.app.bookmarks_changed();
                })
            });
            list.add(&row.row);
        }
    }

    pub fn refresh_history_panel(self: &Rc<Self>) {
        let list = &self.history_panel.list;
        for child in list.children() {
            list.remove(&child);
        }
        let search = self.history_panel.search.as_ref().map(|s| s.text().to_string()).unwrap_or_default();
        for e in self.app.storage.history(&search, 300) {
            let when = glib::DateTime::from_unix_local(e.time)
                .ok()
                .and_then(|d| d.format("%d.%m. %H:%M").ok())
                .map(|s| s.to_string());
            let row = sidebar::entry_row(&e.title, &e.url, when.as_deref());
            let w = self.weak();
            let url = e.url.clone();
            row.open.connect_button_press_event(move |_, event| {
                with(&w, |w| w.open_from_panel(&url, event));
                glib::Propagation::Stop
            });
            let w = self.weak();
            let url = e.url.clone();
            row.remove.connect_clicked(move |_| {
                with(&w, |w| {
                    w.app.storage.delete_history_entry(&url);
                    w.app.history_changed();
                })
            });
            list.add(&row.row);
        }
    }

    pub fn history_changed(self: &Rc<Self>) {
        if self.panel_holder.is_visible() && self.panel_stack.visible_child_name().as_deref() == Some("history") {
            self.refresh_history_panel();
        }
    }

    pub fn bookmarks_changed(self: &Rc<Self>) {
        self.rebuild_favorites();
        if self.panel_holder.is_visible() && self.panel_stack.visible_child_name().as_deref() == Some("bookmarks") {
            self.refresh_bookmarks_panel();
        }
        let uri = self.current_tab().map(|t| t.uri()).unwrap_or_default();
        self.update_entry_icons(&uri);
    }

    fn rebuild_favorites(self: &Rc<Self>) {
        for child in self.favorites.children() {
            self.favorites.remove(&child);
        }
        let bookmarks = self.app.storage.bookmarks();
        if bookmarks.is_empty() {
            let hint = gtk::Label::new(Some("Favoriten hinzufügen: auf den ☆ in der Adressleiste klicken oder Strg+D drücken"));
            hint.style_context().add_class("favorites-hint");
            self.favorites.pack_start(&hint, false, false, 6);
        }
        let favicons = self.app.favicon_database();
        for e in bookmarks.into_iter().rev() {
            let button = gtk::Button::new();
            button.set_relief(gtk::ReliefStyle::None);
            button.style_context().add_class("favorite");
            button.set_tooltip_text(Some(&format!("{}\n{}", e.title, e.url)));
            let bx = gtk::Box::new(Orientation::Horizontal, 6);
            let icon = gtk::Image::from_icon_name(Some("starred-symbolic"), gtk::IconSize::Menu);
            let label = gtk::Label::new(Some(if e.title.trim().is_empty() { &e.url } else { &e.title }));
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(18);
            bx.pack_start(&icon, false, false, 0);
            bx.pack_start(&label, false, false, 0);
            button.add(&bx);
            if let Some(db) = &favicons {
                let icon = icon.clone();
                let scale = self.window.scale_factor();
                webkit2gtk::FaviconDatabaseExt::favicon(db, &e.url, None::<&gio::Cancellable>, move |result| {
                    if let Ok(surface) = result {
                        if let Ok(image) = gtk::cairo::ImageSurface::try_from(surface) {
                            if let Some(pixbuf) = gtk::gdk::pixbuf_get_from_surface(&image, 0, 0, image.width(), image.height()) {
                                let size = 16 * scale;
                                if let Some(scaled) = pixbuf.scale_simple(size, size, gtk::gdk_pixbuf::InterpType::Bilinear) {
                                    icon.set_from_pixbuf(Some(&scaled));
                                }
                            }
                        }
                    }
                });
            }
            let w = self.weak();
            let url = e.url.clone();
            button.connect_button_release_event(move |_, event| {
                with(&w, |w| match event.button() {
                    1 if event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK) => {
                        w.open_tab(&url, true);
                    }
                    1 => w.load_in_current(&url),
                    2 => {
                        w.open_tab(&url, true);
                    }
                    3 => w.favorite_menu(&url, event),
                    _ => {}
                });
                glib::Propagation::Stop
            });
            self.favorites.pack_start(&button, false, false, 0);
        }
        self.favorites.show_all();
    }

    fn favorite_menu(self: &Rc<Self>, url: &str, event: &gtk::gdk::EventButton) {
        let menu = gtk::Menu::new();
        let entries: [(&str, Box<dyn Fn(&Rc<BrowserWindow>, &str)>); 3] = [
            ("In neuem Tab öffnen", Box::new(|w, url| {
                w.open_tab(url, false);
            })),
            ("In privatem Tab öffnen", Box::new(|w, url| {
                w.open_tab_in(url, false, Mode::Private);
            })),
            ("Aus Favoriten entfernen", Box::new(|w, url| {
                w.app.storage.remove_bookmark(url);
                w.app.bookmarks_changed();
            })),
        ];
        for (label, f) in entries {
            let item = gtk::MenuItem::with_label(label);
            let w = self.weak();
            let url = url.to_string();
            item.connect_activate(move |_| with(&w, |w| f(w, &url)));
            menu.append(&item);
        }
        menu.show_all();
        menu.popup_at_pointer(Some(event));
    }

    pub fn layout_changed(self: &Rc<Self>) {
        let config = self.app.config.borrow();
        let full = self.fullscreen.get();
        self.favorites_bar.set_visible(config.show_favorites && !full);
        self.sidebar.set_visible(config.show_sidebar && !full);
        self.strip.set_visible(!full);
        if full || !config.show_sidebar {
            self.panel_holder.hide();
            self.sync_toggles(None);
        }
    }

    pub fn tor_status_changed(&self) {
        self.update_mode_look(self.current_mode());
    }

    fn update_mode_look(&self, mode: Mode) {
        self.titlebar.set_mode(mode);
        match mode {
            Mode::Normal => self.mode_badge.hide(),
            Mode::Private => {
                self.mode_badge.set_text("Privat");
                self.mode_badge.style_context().remove_class("tor-badge");
                self.mode_badge.set_tooltip_text(Some("Privater Tab: kein Verlauf, Cookies werden beim Schließen des letzten privaten Tabs gelöscht"));
                self.mode_badge.show();
            }
            Mode::Tor => {
                let status = self.app.tor.status();
                self.mode_badge.set_text(&format!("Tor · {}", status.label()));
                self.mode_badge.style_context().add_class("tor-badge");
                self.mode_badge.set_tooltip_text(Some(&status.summary));
                self.mode_badge.show();
            }
        }
        self.entry.set_placeholder_text(Some(match mode {
            Mode::Normal => "Suchen oder Adresse eingeben",
            Mode::Private => "Privat suchen oder Adresse eingeben",
            Mode::Tor => "Über Tor suchen oder Adresse eingeben",
        }));
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

fn main_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let windows = gio::Menu::new();
    windows.append(Some("Neuer Tab"), Some("win.new-tab"));
    windows.append(Some("Neues Fenster"), Some("win.new-window"));
    windows.append(Some("Neuer privater Tab"), Some("win.new-private-tab"));
    windows.append(Some("Neuer Tor-Tab"), Some("win.new-tor-tab"));
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

    let view = gio::Menu::new();
    view.append(Some("Favoritenleiste ein/aus"), Some("win.toggle-favorites"));
    view.append(Some("Seitenleiste ein/aus"), Some("win.toggle-sidebar"));
    view.append(Some("Hintergrund-Tabs schlafen legen"), Some("win.sleep-tab"));
    view.append(Some("Engine wechseln (WebKit ↔ Servo)"), Some("win.switch-engine"));
    menu.append_section(None, &view);

    let misc = gio::Menu::new();
    misc.append(Some("Lesezeichen"), Some("win.bookmarks"));
    misc.append(Some("Verlauf"), Some("win.history"));
    misc.append(Some("Downloads"), Some("win.downloads"));
    misc.append(Some("Einstellungen"), Some("win.settings"));
    misc.append(Some("Entwicklerwerkzeuge"), Some("win.inspector"));
    misc.append(Some("Über PoeziBrowse"), Some("win.about"));
    menu.append_section(None, &misc);
    menu
}
