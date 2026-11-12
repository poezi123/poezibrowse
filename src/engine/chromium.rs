use std::cell::{Cell, RefCell};
use std::os::raw::{c_int, c_uint, c_ulong, c_void};
use std::rc::Rc;

use cef::rc::Rc as _;
use cef::*;
use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;

use crate::config::EngineKind;
use crate::engine::{Page, PageEvent, SinkSlot};

#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_x11_window_get_xid(window: *mut c_void) -> c_ulong;
    fn gdk_x11_display_get_xdisplay(display: *mut c_void) -> *mut c_void;
    fn gdk_window_ensure_native(window: *mut c_void) -> c_int;
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XMoveResizeWindow(
        display: *mut c_void,
        window: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    fn XFlush(display: *mut c_void) -> c_int;
}

thread_local! {
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
    static ACCELERATED: Cell<bool> = const { Cell::new(true) };
}

fn cache_dir() -> Option<String> {
    let dir = dirs::cache_dir()?.join("poezibrowse").join("chromium");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.to_string_lossy().into_owned())
}

fn register_api_version() {
    cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
}

wrap_app! {
    pub struct PoeziApp {}

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            command_line.append_switch_with_value(
                Some(&CefString::from("ozone-platform")),
                Some(&CefString::from("x11")),
            );
            if !ACCELERATED.with(Cell::get) {
                command_line.append_switch(Some(&CefString::from("disable-gpu")));
                command_line.append_switch(Some(&CefString::from("disable-gpu-compositing")));
            }
        }
    }
}

pub fn execute_subprocess() -> Option<i32> {
    register_api_version();
    let args = cef::args::Args::new();
    let mut app = PoeziApp::new();
    let code = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 { Some(code) } else { None }
}

pub fn ensure_initialized() -> bool {
    if INITIALIZED.with(Cell::get) {
        return true;
    }
    register_api_version();
    let args = cef::args::Args::new();
    let mut app = PoeziApp::new();
    let mut settings = cef::Settings::default();
    settings.no_sandbox = 1;
    settings.external_message_pump = 0;
    settings.multi_threaded_message_loop = 0;
    settings.windowless_rendering_enabled = 0;
    if let Some(dir) = cache_dir() {
        settings.root_cache_path = CefString::from(dir.as_str());
        settings.cache_path = CefString::from(dir.as_str());
    }
    let rc = cef::initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if rc != 1 {
        return false;
    }
    INITIALIZED.with(|i| i.set(true));
    glib::timeout_add_local(std::time::Duration::from_millis(10), || {
        cef::do_message_loop_work();
        glib::ControlFlow::Continue
    });
    true
}

pub fn shutdown() {
    if INITIALIZED.with(Cell::get) {
        cef::shutdown();
        INITIALIZED.with(|i| i.set(false));
    }
}

struct Inner {
    sink: SinkSlot,
    browser: RefCell<Option<Browser>>,
    title: RefCell<Option<String>>,
    uri: RefCell<Option<String>>,
    loading: Cell<bool>,
    can_back: Cell<bool>,
    can_fwd: Cell<bool>,
    zoom: Cell<f64>,
    pending_uri: RefCell<Option<String>>,
    created: Cell<bool>,
    size: Cell<(i32, i32)>,
}

impl Inner {
    fn new(zoom: f64) -> Self {
        Inner {
            sink: SinkSlot::default(),
            browser: RefCell::new(None),
            title: RefCell::new(None),
            uri: RefCell::new(None),
            loading: Cell::new(false),
            can_back: Cell::new(false),
            can_fwd: Cell::new(false),
            zoom: Cell::new(zoom),
            pending_uri: RefCell::new(None),
            created: Cell::new(false),
            size: Cell::new((1, 1)),
        }
    }
}

fn default_xdisplay() -> Option<*mut c_void> {
    let display = gtk::gdk::Display::default()?;
    let display_native: *mut gtk::gdk::ffi::GdkDisplay = display.to_glib_none().0;
    if display_native.is_null() {
        return None;
    }
    let xdisplay = unsafe { gdk_x11_display_get_xdisplay(display_native as *mut c_void) };
    if xdisplay.is_null() { None } else { Some(xdisplay) }
}

fn apply_cef_size(inner: &Rc<Inner>) {
    let borrow = inner.browser.borrow();
    let Some(browser) = borrow.as_ref() else { return };
    let Some(host) = browser.host() else { return };
    let xid = host.window_handle();
    if xid == 0 {
        return;
    }
    let Some(xdisplay) = default_xdisplay() else { return };
    let (width, height) = inner.size.get();
    unsafe {
        XMoveResizeWindow(xdisplay, xid, 0, 0, width.max(1) as c_uint, height.max(1) as c_uint);
        XFlush(xdisplay);
    }
}

fn cef_str(value: Option<&CefString>) -> Option<String> {
    value.and_then(|s| CefStringUtf8::from(s).as_str().map(str::to_owned))
}

wrap_life_span_handler! {
    pub struct PoeziLifeSpan {
        inner: Rc<Inner>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            self.inner.browser.replace(Some(browser.clone()));
            let level = self.inner.zoom.get();
            if (level - 1.0).abs() > f64::EPSILON {
                if let Some(host) = browser.host() {
                    host.set_zoom_level(level.ln() / 1.2f64.ln());
                }
            }
            if let Some(uri) = self.inner.pending_uri.borrow_mut().take() {
                if let Some(frame) = browser.main_frame() {
                    frame.load_url(Some(&CefString::from(uri.as_str())));
                }
            }
            apply_cef_size(&self.inner);
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> ::std::os::raw::c_int {
            0
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            self.inner.browser.replace(None);
            self.inner.sink.emit(PageEvent::CloseRequested);
        }
    }
}

wrap_load_handler! {
    pub struct PoeziLoad {
        inner: Rc<Inner>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let loading = is_loading != 0;
            self.inner.loading.set(loading);
            self.inner.can_back.set(can_go_back != 0);
            self.inner.can_fwd.set(can_go_forward != 0);
            self.inner.sink.emit(PageEvent::LoadingChanged);
            self.inner.sink.emit(PageEvent::NavigationStateChanged);
            if !loading {
                self.inner.sink.emit(PageEvent::Progress(1.0));
                self.inner.sink.emit(PageEvent::LoadFinished);
            }
        }

        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            self.inner.sink.emit(PageEvent::Progress(0.1));
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _error_code: Errorcode,
            _error_text: Option<&CefString>,
            _failed_url: Option<&CefString>,
        ) {
            self.inner.loading.set(false);
            self.inner.sink.emit(PageEvent::LoadingChanged);
        }
    }
}

wrap_display_handler! {
    pub struct PoeziDisplay {
        inner: Rc<Inner>,
    }

    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            self.inner.title.replace(cef_str(title));
            self.inner.sink.emit(PageEvent::TitleChanged);
        }

        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            self.inner.uri.replace(cef_str(url));
            self.inner.sink.emit(PageEvent::UriChanged);
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            self.inner.sink.emit(PageEvent::Progress(progress));
        }
    }
}

wrap_client! {
    pub struct PoeziClient {
        life: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }
    }
}

fn spawn_browser(inner: &Rc<Inner>, parent: c_ulong, width: i32, height: i32) {
    if inner.created.get() {
        return;
    }
    let life = PoeziLifeSpan::new(inner.clone());
    let load = PoeziLoad::new(inner.clone());
    let display = PoeziDisplay::new(inner.clone());
    let mut client = PoeziClient::new(life, load, display);

    let url = inner.pending_uri.borrow().clone();
    let url_cef = url.as_deref().map(CefString::from);
    let bounds = Rect {
        x: 0,
        y: 0,
        width: width.max(1),
        height: height.max(1),
    };
    let mut window_info = WindowInfo::default().set_as_child(parent, &bounds);
    window_info.runtime_style = RuntimeStyle::ALLOY;
    let browser_settings = BrowserSettings::default();

    let ok = cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut client),
        url_cef.as_ref(),
        Some(&browser_settings),
        None,
        None,
    );
    inner.created.set(ok == 1);
    if ok == 1 {
        inner.pending_uri.replace(None);
    }
}

pub struct ChromiumPage {
    area: gtk::DrawingArea,
    inner: Rc<Inner>,
}

impl ChromiumPage {
    pub fn new(uri: &str, zoom: f64, accelerated: bool) -> Option<Box<dyn Page>> {
        ACCELERATED.with(|a| a.set(accelerated));
        if !ensure_initialized() {
            return None;
        }
        let inner = Rc::new(Inner::new(zoom));
        if !uri.is_empty() {
            inner.pending_uri.replace(Some(uri.to_string()));
        }

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_can_focus(true);
        area.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);

        let realize_inner = inner.clone();
        area.connect_realize(move |area| {
            let Some(window) = area.window() else { return };
            let window_native: *mut gtk::gdk::ffi::GdkWindow = window.to_glib_none().0;
            let window_ptr = window_native as *mut c_void;
            if window_ptr.is_null() {
                return;
            }
            unsafe { gdk_window_ensure_native(window_ptr) };
            let xid = unsafe { gdk_x11_window_get_xid(window_ptr) };
            if xid == 0 {
                return;
            }
            let alloc = area.allocation();
            realize_inner.size.set((alloc.width(), alloc.height()));
            spawn_browser(&realize_inner, xid, alloc.width(), alloc.height());
        });

        let resize_inner = inner.clone();
        area.connect_size_allocate(move |_area, alloc| {
            resize_inner.size.set((alloc.width(), alloc.height()));
            apply_cef_size(&resize_inner);
        });

        let focus_inner = inner.clone();
        area.connect_button_press_event(move |_area, _event| {
            if let Some(browser) = focus_inner.browser.borrow().as_ref() {
                if let Some(host) = browser.host() {
                    host.set_focus(1);
                }
            }
            glib::Propagation::Proceed
        });

        Some(Box::new(ChromiumPage { area, inner }))
    }

    fn with_browser<R>(&self, f: impl FnOnce(&Browser) -> R) -> Option<R> {
        self.inner.browser.borrow().as_ref().map(f)
    }
}

impl Page for ChromiumPage {
    fn engine(&self) -> EngineKind {
        EngineKind::Chromium
    }

    fn widget(&self) -> gtk::Widget {
        self.area.clone().upcast()
    }

    fn set_event_sink(&self, sink: crate::engine::EventSink) {
        self.inner.sink.set(sink);
    }

    fn load_uri(&self, uri: &str) {
        let loaded = self
            .with_browser(|browser| {
                if let Some(frame) = browser.main_frame() {
                    frame.load_url(Some(&CefString::from(uri)));
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if !loaded {
            self.inner.pending_uri.replace(Some(uri.to_string()));
        }
    }

    fn reload(&self, bypass_cache: bool) {
        self.with_browser(|browser| {
            if bypass_cache {
                browser.reload_ignore_cache();
            } else {
                browser.reload();
            }
        });
    }

    fn stop(&self) {
        self.with_browser(Browser::stop_load);
    }

    fn go_back(&self) {
        self.with_browser(Browser::go_back);
    }

    fn go_forward(&self) {
        self.with_browser(Browser::go_forward);
    }

    fn uri(&self) -> Option<String> {
        self.inner.uri.borrow().clone()
    }

    fn title(&self) -> Option<String> {
        self.inner.title.borrow().clone()
    }

    fn can_go_back(&self) -> bool {
        self.inner.can_back.get()
    }

    fn can_go_forward(&self) -> bool {
        self.inner.can_fwd.get()
    }

    fn is_loading(&self) -> bool {
        self.inner.loading.get()
    }

    fn zoom(&self) -> f64 {
        self.inner.zoom.get()
    }

    fn set_zoom(&self, level: f64) {
        self.inner.zoom.set(level);
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                host.set_zoom_level(level.ln() / 1.2f64.ln());
            }
        });
    }

    fn print(&self, _parent: &gtk::Window) {
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                host.print();
            }
        });
    }

    fn show_inspector(&self) {
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                host.show_dev_tools(None, None, None, None);
            }
        });
    }

    fn close(&self) {
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        });
    }
}
