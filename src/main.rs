mod app;
mod blocker;
mod config;
mod cookies;
mod countries;
mod engine;
mod intercept;
mod memory;
mod pages;
mod passwords;
mod session;
mod sidebar;
mod storage;
mod system;
mod tab;
mod terminal;
mod titlebar;
mod tor;
mod window;
mod youtube;

use gtk::gio;
use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    let startup_config = config::Config::load();
    let nvidia = system::nvidia();
    if nvidia && std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
        unsafe { std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1") };
    }
    if nvidia && system::wayland_session() && !system::webkit_at_least(2, 54) && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    {
        let set = |k: &str, v: &str| {
            if std::env::var_os(k).is_none() {
                unsafe { std::env::set_var(k, v) };
            }
        };
        let safe_sink = || {
            set("WEBKIT_GST_DMABUF_SINK_DISABLED", "1");
            set("WEBKIT_GST_DISABLE_GL_SINK", "1");
        };
        let software_decoders = || {
            set(
                "GST_PLUGIN_FEATURE_RANK",
                "nvh264dec:0,nvh264sldec:0,nvh265dec:0,nvh265sldec:0,nvvp8dec:0,nvvp9dec:0,nvvp9sldec:0,nvav1dec:0,\
                 vah264dec:0,vah265dec:0,vavp8dec:0,vavp9dec:0,vaav1dec:0,\
                 vaapih264dec:0,vaapih265dec:0,vaapivp8dec:0,vaapivp9dec:0,vaapiav1dec:0",
            );
        };
        match startup_config.video_mode {
            config::VideoMode::Auto => {
                if nvidia {
                    safe_sink();
                }
            }
            config::VideoMode::Compatible => safe_sink(),
            config::VideoMode::Software => {
                safe_sink();
                software_decoders();
            }
            config::VideoMode::Gpu => {}
        }
    }
    if !startup_config.javascript_jit && std::env::var_os("JSC_useJIT").is_none() {
        unsafe { std::env::set_var("JSC_useJIT", "false") };
    }
    if std::env::args().nth(1).as_deref() == Some("--diagnose") {
        print!("{}", system::diagnose());
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::args().nth(1).as_deref() == Some(blocker::BUILD_FILTERS_ARG) {
        return if blocker::build_filter_files() { gtk::glib::ExitCode::SUCCESS } else { gtk::glib::ExitCode::FAILURE };
    }

    let gtk_app = gtk::Application::new(Some(config::APP_ID), gio::ApplicationFlags::HANDLES_OPEN);
    gtk::glib::set_application_name(config::APP_NAME);
    gtk::glib::set_prgname(Some("poezibrowse"));

    let app = App::new(&gtk_app);
    let a = app.clone();
    gtk_app.connect_startup(move |gtk_app| {
        set_accels(gtk_app);
        for (name, mode) in [("new-window", config::Mode::Normal), ("new-private-tab", config::Mode::Private), ("new-tor-tab", config::Mode::Tor)] {
            let action = gio::SimpleAction::new(name, None);
            let a = a.clone();
            action.connect_activate(move |_, _| {
                if mode == config::Mode::Normal {
                    a.new_window(true);
                } else {
                    a.open_mode_tab(mode);
                }
            });
            gtk_app.add_action(&action);
        }
        a.start();
    });
    gtk_app.add_main_option("new-window", gtk::glib::Char::from(b'n'), gtk::glib::OptionFlags::NONE, gtk::glib::OptionArg::None, "Neues Fenster öffnen", None);
    gtk_app.add_main_option("private", gtk::glib::Char::from(b'p'), gtk::glib::OptionFlags::NONE, gtk::glib::OptionArg::None, "Neuen privaten Tab öffnen", None);
    gtk_app.add_main_option("tor", gtk::glib::Char::from(b't'), gtk::glib::OptionFlags::NONE, gtk::glib::OptionArg::None, "Neuen Tor-Tab öffnen", None);
    let a = app.clone();
    gtk_app.connect_handle_local_options(move |gtk_app, options| {
        let action = if options.contains("tor") {
            "new-tor-tab"
        } else if options.contains("private") {
            "new-private-tab"
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
        ("win.new-private-tab", &["<Primary><Shift>p"]),
        ("win.new-tor-tab", &["<Primary><Shift>u"]),
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
