mod app;
mod blocker;
mod config;
mod engine;
mod memory;
mod pages;
mod passwords;
mod session;
mod sidebar;
mod storage;
mod tab;
mod titlebar;
mod tor;
mod window;
mod youtube;

use gtk::gio;
use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    if std::env::args().nth(1).as_deref() == Some(blocker::BUILD_FILTERS_ARG) {
        return if blocker::build_filter_files() { gtk::glib::ExitCode::SUCCESS } else { gtk::glib::ExitCode::FAILURE };
    }
    if std::path::Path::new("/proc/driver/nvidia").exists() && std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
        unsafe { std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1") };
    }

    let gtk_app = gtk::Application::new(Some(config::APP_ID), gio::ApplicationFlags::HANDLES_OPEN);
    gtk::glib::set_application_name(config::APP_NAME);
    gtk::glib::set_prgname(Some("poezibrowse"));

    let app = App::new(&gtk_app);
    let a = app.clone();
    gtk_app.connect_startup(move |gtk_app| {
        set_accels(gtk_app);
        for (name, mode) in [("new-window", config::Mode::Normal), ("new-private-window", config::Mode::Private), ("new-tor-window", config::Mode::Tor)] {
            let action = gio::SimpleAction::new(name, None);
            let a = a.clone();
            action.connect_activate(move |_, _| {
                a.new_window(mode, true);
            });
            gtk_app.add_action(&action);
        }
        a.start();
    });
    gtk_app.add_main_option("new-window", gtk::glib::Char::from(b'n'), gtk::glib::OptionFlags::NONE, gtk::glib::OptionArg::None, "Neues Fenster öffnen", None);
    gtk_app.add_main_option("private-window", gtk::glib::Char::from(b'p'), gtk::glib::OptionFlags::NONE, gtk::glib::OptionArg::None, "Neues privates Fenster öffnen", None);
    gtk_app.add_main_option("tor-window", gtk::glib::Char::from(b't'), gtk::glib::OptionFlags::NONE, gtk::glib::OptionArg::None, "Neues Tor-Fenster öffnen", None);
    let a = app.clone();
    gtk_app.connect_handle_local_options(move |gtk_app, options| {
        let action = if options.contains("tor-window") {
            "new-tor-window"
        } else if options.contains("private-window") {
            "new-private-window"
        } else if options.contains("new-window") {
            "new-window"
        } else {
            return -1;
        };
        if gtk_app.register(None::<&gio::Cancellable>).is_err() {
            return 1;
        }
        if gtk_app.is_remote() {
            gtk_app.activate_action(action, None);
            return 0;
        }
        a.set_start_action(action);
        -1
    });
    let a = app.clone();
    gtk_app.connect_activate(move |_| a.activate());
    let a = app.clone();
    gtk_app.connect_open(move |_, files, _| {
        let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
        a.open_uris(&uris);
    });
    for signal in [1, 2, 15] {
        let gtk_app = gtk_app.clone();
        gtk::glib::unix_signal_add_local(signal, move || {
            gtk_app.quit();
            gtk::glib::ControlFlow::Break
        });
    }
    let code = gtk_app.run();
    drop(app);
    code
}

use app::App;

fn set_accels(app: &gtk::Application) {
    let accels: &[(&str, &[&str])] = &[
        ("win.new-tab", &["<Primary>t"]),
        ("win.close-tab", &["<Primary>w", "<Primary>F4"]),
        ("win.reopen-tab", &["<Primary><Shift>t"]),
        ("win.new-window", &["<Primary>n"]),
        ("win.new-private-window", &["<Primary><Shift>p"]),
        ("win.new-tor-window", &["<Primary><Shift>u"]),
        ("win.move-tab-left", &["<Primary><Shift>Page_Up"]),
        ("win.move-tab-right", &["<Primary><Shift>Page_Down"]),
        ("win.toggle-sidebar", &["F9"]),
        ("win.focus-location", &["<Primary>l", "<Alt>d", "F6"]),
        ("win.reload", &["<Primary>r", "F5"]),
        ("win.reload-hard", &["<Primary><Shift>r", "<Shift>F5"]),
        ("win.back", &["<Alt>Left", "Back"]),
        ("win.forward", &["<Alt>Right", "Forward"]),
        ("win.next-tab", &["<Primary>Tab", "<Primary>Page_Down"]),
        ("win.prev-tab", &["<Primary><Shift>Tab", "<Primary><Shift>ISO_Left_Tab", "<Primary>Page_Up"]),
        ("win.find", &["<Primary>f"]),
        ("win.zoom-in", &["<Primary>plus", "<Primary>equal", "<Primary>KP_Add"]),
        ("win.zoom-out", &["<Primary>minus", "<Primary>KP_Subtract"]),
        ("win.zoom-reset", &["<Primary>0", "<Primary>KP_0"]),
        ("win.print", &["<Primary>p"]),
        ("win.inspector", &["<Primary><Shift>i", "F12"]),
        ("win.fullscreen", &["F11"]),
        ("win.downloads", &["<Primary><Shift>y"]),
        ("win.bookmark", &["<Primary>d"]),
        ("win.history", &["<Primary>h"]),
        ("win.bookmarks", &["<Primary><Shift>o"]),
    ];
    for (action, keys) in accels {
        app.set_accels_for_action(action, keys);
    }
    for n in 1..=9 {
        app.set_accels_for_action(&format!("win.select-tab({n})"), &[&format!("<Primary>{n}")]);
    }
}
