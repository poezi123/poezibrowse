use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

use gtk::glib;
use gtk::glib::translate::from_glib_none;
use gtk::prelude::*;

#[allow(non_camel_case_types)]
type GtkWidget = gtk::ffi::GtkWidget;

#[link(name = "vte-2.91")]
unsafe extern "C" {
    fn vte_terminal_new() -> *mut GtkWidget;
    fn vte_terminal_set_scrollback_lines(terminal: *mut GtkWidget, lines: c_int);
    fn vte_terminal_paste_clipboard(terminal: *mut GtkWidget);
    fn vte_terminal_copy_clipboard_format(terminal: *mut GtkWidget, format: c_int);
    fn vte_terminal_spawn_sync(
        terminal: *mut GtkWidget,
        pty_flags: c_int,
        working_directory: *const c_char,
        argv: *const *mut c_char,
        envv: *const *mut c_char,
        spawn_flags: c_int,
        child_setup: Option<unsafe extern "C" fn()>,
        child_setup_data: *mut c_void,
        child_pid: *mut c_int,
        cancellable: *mut c_void,
        error: *mut *mut c_void,
    ) -> c_int;
}

fn spawn_shell(terminal: *mut GtkWidget) {
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/bash".to_string());
    let Ok(c_shell) = std::ffi::CString::new(shell) else { return };
    let argv: [*mut c_char; 2] = [c_shell.as_ptr() as *mut c_char, ptr::null_mut()];
    let home = dirs::home_dir().and_then(|h| std::ffi::CString::new(h.to_string_lossy().as_bytes()).ok());
    let workdir = home.as_ref().map(|h| h.as_ptr()).unwrap_or(ptr::null());
    unsafe {
        vte_terminal_spawn_sync(
            terminal,
            0,
            workdir,
            argv.as_ptr(),
            ptr::null(),
            4,
            None,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }
}

pub fn widget() -> gtk::Widget {
    unsafe {
        let raw = vte_terminal_new();
        vte_terminal_set_scrollback_lines(raw, 20000);
        let widget: gtk::Widget = from_glib_none(raw);
        spawn_shell(raw);
        let raw_ptr = raw as usize;
        widget.connect_local("child-exited", false, move |_| {
            spawn_shell(raw_ptr as *mut GtkWidget);
            None
        });
        let raw_ptr = raw as usize;
        widget.connect_key_press_event(move |_, event| {
            let state = event.state();
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let key = event.keyval();
            let term = raw_ptr as *mut GtkWidget;
            if ctrl && (key == gtk::gdk::keys::constants::v || key == gtk::gdk::keys::constants::V) {
                vte_terminal_paste_clipboard(term);
                return glib::Propagation::Stop;
            }
            if ctrl && shift && (key == gtk::gdk::keys::constants::c || key == gtk::gdk::keys::constants::C) {
                vte_terminal_copy_clipboard_format(term, 1);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        widget
    }
}

pub fn pane() -> gtk::Widget {
    let term = widget();
    term.set_hexpand(true);
    term.set_vexpand(true);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    scroller.add(&term);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    let header = gtk::Label::new(Some("Terminal"));
    header.set_xalign(0.0);
    header.style_context().add_class("tool-title");
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bx.set_size_request(360, -1);
    bx.pack_start(&header, false, false, 0);
    bx.pack_start(&scroller, true, true, 0);
    bx.show_all();
    bx.upcast()
}
