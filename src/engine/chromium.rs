use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cef::rc::Rc as _;
use cef::*;
use gtk::cairo;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

use crate::config::EngineKind;
use crate::engine::{Page, PageEvent, SinkSlot};

const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
const EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
const EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;

thread_local! {
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
    static PUMPING: Cell<bool> = const { Cell::new(false) };
    static PUMP_AGAIN: Cell<bool> = const { Cell::new(false) };
}

static PUMP_PENDING: AtomicBool = AtomicBool::new(false);

fn pump() {
    PUMP_PENDING.store(false, Ordering::Release);
    if !INITIALIZED.with(Cell::get) {
        return;
    }
    if PUMPING.with(Cell::get) {
        PUMP_AGAIN.with(|c| c.set(true));
        return;
    }
    PUMPING.with(|c| c.set(true));
    cef::do_message_loop_work();
    PUMPING.with(|c| c.set(false));
    if PUMP_AGAIN.with(|c| c.replace(false)) {
        schedule_pump(0);
    }
}

fn schedule_pump(delay_ms: i64) {
    if delay_ms <= 0 {
        if !PUMP_PENDING.swap(true, Ordering::AcqRel) {
            glib::timeout_add_once(std::time::Duration::ZERO, pump);
        }
    } else {
        glib::timeout_add_once(std::time::Duration::from_millis(delay_ms.min(50) as u64), pump);
    }
}

fn target_frame_rate() -> i32 {
    let Some(display) = gdk::Display::default() else { return 60 };
    let mut best = 60;
    for i in 0..display.n_monitors() {
        if let Some(monitor) = display.monitor(i) {
            best = best.max(monitor.refresh_rate() / 1000);
        }
    }
    best.clamp(60, 120)
}

wrap_browser_process_handler! {
    pub struct PoeziBrowserProcess {}

    impl BrowserProcessHandler {
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            schedule_pump(delay_ms);
        }
    }
}

fn cef_resource_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.to_path_buf(),
        exe_dir.join("../lib/poezibrowse"),
        std::path::PathBuf::from("/usr/lib/cef"),
    ];
    for dir in candidates {
        if dir.join("icudtl.dat").is_file() {
            return dir.canonicalize().ok();
        }
    }
    None
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
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(PoeziBrowserProcess::new())
        }

        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            let ozone = if std::env::var_os("WAYLAND_DISPLAY").is_some() { "wayland" } else { "x11" };
            command_line.append_switch_with_value(
                Some(&CefString::from("ozone-platform")),
                Some(&CefString::from(ozone)),
            );
            command_line.append_switch(Some(&CefString::from("enable-smooth-scrolling")));
            if std::env::var_os("POEZI_CEF_GPU").as_deref() == Some(std::ffi::OsStr::new("1")) {
                command_line.append_switch(Some(&CefString::from("ignore-gpu-blocklist")));
                command_line.append_switch(Some(&CefString::from("enable-gpu-rasterization")));
                if std::path::Path::new("/proc/driver/nvidia").exists() {
                    command_line.append_switch_with_value(
                        Some(&CefString::from("use-angle")),
                        Some(&CefString::from("vulkan")),
                    );
                    command_line.append_switch_with_value(
                        Some(&CefString::from("enable-features")),
                        Some(&CefString::from("Vulkan,DefaultANGLEVulkan,VulkanFromANGLE")),
                    );
                }
            } else {
                command_line.append_switch(Some(&CefString::from("disable-gpu")));
                command_line.append_switch(Some(&CefString::from("disable-gpu-compositing")));
            }
            command_line.append_switch(Some(&CefString::from("no-sandbox")));
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
    settings.external_message_pump = 1;
    settings.multi_threaded_message_loop = 0;
    settings.windowless_rendering_enabled = 1;
    if std::env::var_os("POEZI_CEF_LOG").is_some() {
        settings.log_severity = LogSeverity::INFO;
        settings.log_file = CefString::from("/tmp/poezi-cef.log");
    }
    if let Some(dir) = cache_dir() {
        settings.root_cache_path = CefString::from(dir.as_str());
        settings.cache_path = CefString::from(dir.as_str());
    }
    if let Some(dir) = cef_resource_dir() {
        let dir = dir.to_string_lossy().into_owned();
        settings.resources_dir_path = CefString::from(dir.as_str());
        settings.locales_dir_path = CefString::from(format!("{dir}/locales").as_str());
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
    schedule_pump(0);
    glib::timeout_add_local(std::time::Duration::from_millis(50), || {
        if !INITIALIZED.with(Cell::get) {
            return glib::ControlFlow::Break;
        }
        pump();
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

enum BridgeMsg {
    Log { method: String, url: String },
    Hold {
        method: String,
        url: String,
        callback: Callback,
        browser: Option<Browser>,
    },
}

struct Bridge {
    log_requests: AtomicBool,
    intercepting: AtomicBool,
    queue: Mutex<VecDeque<BridgeMsg>>,
}

impl Bridge {
    fn new() -> Self {
        Bridge {
            log_requests: AtomicBool::new(false),
            intercepting: AtomicBool::new(false),
            queue: Mutex::new(VecDeque::new()),
        }
    }
}

struct Inner {
    sink: SinkSlot,
    browser: RefCell<Option<Browser>>,
    title: RefCell<Option<String>>,
    uri: RefCell<Option<String>>,
    loading: Cell<bool>,
    bridge: Arc<Bridge>,
    on_intercept: RefCell<Option<crate::engine::InterceptRoute>>,
    can_back: Cell<bool>,
    can_fwd: Cell<bool>,
    zoom: Cell<f64>,
    pending_uri: RefCell<Option<String>>,
    mapped: Cell<bool>,
    size: Cell<(i32, i32)>,
    surface: RefCell<Option<cairo::ImageSurface>>,
    area: RefCell<Option<glib::WeakRef<gtk::DrawingArea>>>,
}

impl Inner {
    fn new(zoom: f64) -> Self {
        Inner {
            sink: SinkSlot::default(),
            browser: RefCell::new(None),
            title: RefCell::new(None),
            uri: RefCell::new(None),
            loading: Cell::new(false),
            bridge: Arc::new(Bridge::new()),
            on_intercept: RefCell::new(None),
            can_back: Cell::new(false),
            can_fwd: Cell::new(false),
            zoom: Cell::new(zoom),
            pending_uri: RefCell::new(None),
            mapped: Cell::new(false),
            size: Cell::new((800, 600)),
            surface: RefCell::new(None),
            area: RefCell::new(None),
        }
    }

    fn queue_draw(&self) {
        if let Some(weak) = self.area.borrow().as_ref() {
            if let Some(area) = weak.upgrade() {
                area.queue_draw();
            }
        }
    }

    fn queue_draw_area(&self, x: i32, y: i32, width: i32, height: i32) {
        if let Some(weak) = self.area.borrow().as_ref() {
            if let Some(area) = weak.upgrade() {
                area.queue_draw_area(x, y, width, height);
            }
        }
    }
}

fn cef_str(value: Option<&CefString>) -> Option<String> {
    value.and_then(|s| CefStringUtf8::from(s).as_str().map(str::to_owned))
}

fn userfree_str(value: CefStringUserfree) -> Option<String> {
    let utf16 = CefStringUtf16::from(&value);
    CefStringUtf8::from(&utf16).as_str().map(str::to_owned)
}

fn host_of(inner: &Rc<Inner>) -> Option<BrowserHost> {
    let browser = inner.browser.borrow().clone();
    browser.and_then(|b| b.host())
}

fn drain_bridge(inner: &Rc<Inner>) {
    let msgs: Vec<BridgeMsg> = match inner.bridge.queue.lock() {
        Ok(mut q) => q.drain(..).collect(),
        Err(_) => return,
    };
    for msg in msgs {
        match msg {
            BridgeMsg::Log { method, url } => {
                inner.sink.emit(PageEvent::Request(crate::intercept::RequestRecord {
                    method,
                    url,
                    headers: Vec::new(),
                }));
            }
            BridgeMsg::Hold {
                method,
                url,
                callback,
                browser,
            } => {
                let route = inner.on_intercept.borrow().clone();
                let held = crate::engine::Held::new(method, url, move |d| match d {
                    crate::engine::HeldDecision::Forward => callback.cont(),
                    crate::engine::HeldDecision::Drop => callback.cancel(),
                    crate::engine::HeldDecision::ForwardTo(new_url) => {
                        callback.cancel();
                        if let Some(browser) = browser {
                            if let Some(frame) = browser.main_frame() {
                                frame.load_url(Some(&CefString::from(new_url.as_str())));
                            }
                        }
                    }
                });
                match route {
                    Some(route) => route(held),
                    None => held.forward(),
                }
            }
        }
    }
}

fn try_load(inner: &Rc<Inner>) {
    if !inner.mapped.get() {
        return;
    }
    let borrow = inner.browser.borrow();
    let Some(browser) = borrow.as_ref() else { return };
    let uri = inner.pending_uri.borrow_mut().take();
    if let Some(uri) = uri {
        if let Some(frame) = browser.main_frame() {
            frame.load_url(Some(&CefString::from(uri.as_str())));
        }
    }
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
            if let Some(host) = browser.host() {
                host.set_windowless_frame_rate(target_frame_rate());
                host.was_hidden(if self.inner.mapped.get() { 0 } else { 1 });
                host.was_resized();
            }
            try_load(&self.inner);
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

        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: ::std::os::raw::c_ulong,
            type_: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            let name = if type_ == CursorType::HAND {
                "pointer"
            } else if type_ == CursorType::IBEAM {
                "text"
            } else if type_ == CursorType::CROSS {
                "crosshair"
            } else if type_ == CursorType::WAIT {
                "wait"
            } else if type_ == CursorType::HELP {
                "help"
            } else if type_ == CursorType::MOVE {
                "move"
            } else {
                "default"
            };
            if let Some(area) = self.inner.area.borrow().as_ref().and_then(glib::WeakRef::upgrade) {
                if let Some(window) = area.window() {
                    let display = area.display();
                    let cursor = gdk::Cursor::from_name(&display, name);
                    window.set_cursor(cursor.as_ref());
                }
            }
            1
        }
    }
}

wrap_render_handler! {
    pub struct PoeziRender {
        inner: Rc<Inner>,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let Some(rect) = rect else { return };
            let (w, h) = self.inner.size.get();
            rect.x = 0;
            rect.y = 0;
            rect.width = w.max(1);
            rect.height = h.max(1);
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ != PaintElementType::VIEW {
                return;
            }
            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let stride = width * 4;
            let len = (stride * height) as usize;
            let src = unsafe { std::slice::from_raw_parts(buffer, len) };
            let mut cell = self.inner.surface.borrow_mut();
            let reuse = cell
                .as_ref()
                .is_some_and(|s| s.width() == width && s.height() == height);
            let rects: Vec<(i32, i32, i32, i32)> = dirty_rects
                .unwrap_or(&[])
                .iter()
                .filter_map(|r| {
                    let x = r.x.clamp(0, width);
                    let y = r.y.clamp(0, height);
                    let w = (r.x + r.width).clamp(0, width) - x;
                    let h = (r.y + r.height).clamp(0, height) - y;
                    (w > 0 && h > 0).then_some((x, y, w, h))
                })
                .collect();
            let partial = reuse && !rects.is_empty();
            if reuse {
                if let Some(surface) = cell.as_mut() {
                    surface.flush();
                    if let Ok(mut sdata) = surface.data() {
                        if partial {
                            for &(x, y, w, h) in &rects {
                                for row in y..y + h {
                                    let start = (row * stride + x * 4) as usize;
                                    let end = start + (w * 4) as usize;
                                    sdata[start..end].copy_from_slice(&src[start..end]);
                                }
                            }
                        } else {
                            sdata.copy_from_slice(src);
                        }
                    }
                    surface.mark_dirty();
                }
            } else {
                let data = src.to_vec();
                if let Ok(surface) =
                    cairo::ImageSurface::create_for_data(data, cairo::Format::ARgb32, width, height, stride)
                {
                    *cell = Some(surface);
                }
            }
            drop(cell);
            if partial {
                for (x, y, w, h) in rects {
                    self.inner.queue_draw_area(x, y, w, h);
                }
            } else {
                self.inner.queue_draw();
            }
        }
    }
}

wrap_resource_request_handler! {
    pub struct PoeziResourceRequest {
        bridge: Arc<Bridge>,
        _is_navigation: bool,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let Some(request) = request else {
                return ReturnValue::CONTINUE;
            };
            let url = userfree_str(request.url()).unwrap_or_default();
            if url.is_empty() || url.starts_with("data:") || url.starts_with("blob:") {
                return ReturnValue::CONTINUE;
            }
            let method = userfree_str(request.method()).unwrap_or_else(|| "GET".into());
            if self.bridge.log_requests.load(Ordering::Relaxed) {
                if let Ok(mut q) = self.bridge.queue.lock() {
                    q.push_back(BridgeMsg::Log {
                        method: method.clone(),
                        url: url.clone(),
                    });
                }
            }
            let is_web = url.starts_with("http://") || url.starts_with("https://");
            if is_web && self.bridge.intercepting.load(Ordering::Relaxed) {
                if let Some(callback) = callback {
                    let callback = callback.clone();
                    let browser = browser.map(|b| b.clone());
                    if let Ok(mut q) = self.bridge.queue.lock() {
                        q.push_back(BridgeMsg::Hold {
                            method,
                            url,
                            callback,
                            browser,
                        });
                    }
                    return ReturnValue::CONTINUE_ASYNC;
                }
            }
            ReturnValue::CONTINUE
        }
    }
}

wrap_request_handler! {
    pub struct PoeziRequest {
        bridge: Arc<Bridge>,
    }

    impl RequestHandler {
        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(PoeziResourceRequest::new(self.bridge.clone(), is_navigation != 0))
        }
    }
}

wrap_client! {
    pub struct PoeziClient {
        life: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
        render: RenderHandler,
        request: RequestHandler,
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

        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(self.request.clone())
        }
    }
}

fn spawn_browser(inner: &Rc<Inner>) {
    let life = PoeziLifeSpan::new(inner.clone());
    let load = PoeziLoad::new(inner.clone());
    let display = PoeziDisplay::new(inner.clone());
    let render = PoeziRender::new(inner.clone());
    let request = PoeziRequest::new(inner.bridge.clone());
    let mut client = PoeziClient::new(life, load, display, render, request);

    let mut window_info = WindowInfo::default().set_as_windowless(0);
    window_info.runtime_style = RuntimeStyle::ALLOY;
    let browser_settings = BrowserSettings::default();

    cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut client),
        None,
        Some(&browser_settings),
        None,
        None,
    );
}

fn mods_from_state(state: gdk::ModifierType) -> u32 {
    let mut m = 0;
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        m |= EVENTFLAG_SHIFT_DOWN;
    }
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        m |= EVENTFLAG_CONTROL_DOWN;
    }
    if state.contains(gdk::ModifierType::MOD1_MASK) {
        m |= EVENTFLAG_ALT_DOWN;
    }
    if state.contains(gdk::ModifierType::BUTTON1_MASK) {
        m |= EVENTFLAG_LEFT_MOUSE_BUTTON;
    }
    if state.contains(gdk::ModifierType::BUTTON2_MASK) {
        m |= EVENTFLAG_MIDDLE_MOUSE_BUTTON;
    }
    if state.contains(gdk::ModifierType::BUTTON3_MASK) {
        m |= EVENTFLAG_RIGHT_MOUSE_BUTTON;
    }
    m
}

fn vk_from_keyval(key: &gdk::keys::Key) -> i32 {
    use gdk::keys::constants as k;
    match *key {
        k::Return | k::KP_Enter | k::ISO_Enter => 0x0D,
        k::BackSpace => 0x08,
        k::Tab | k::ISO_Left_Tab => 0x09,
        k::Escape => 0x1B,
        k::Delete | k::KP_Delete => 0x2E,
        k::Insert | k::KP_Insert => 0x2D,
        k::Left | k::KP_Left => 0x25,
        k::Up | k::KP_Up => 0x26,
        k::Right | k::KP_Right => 0x27,
        k::Down | k::KP_Down => 0x28,
        k::Home | k::KP_Home => 0x24,
        k::End | k::KP_End => 0x23,
        k::Page_Up | k::KP_Page_Up => 0x21,
        k::Page_Down | k::KP_Page_Down => 0x22,
        k::space => 0x20,
        k::Control_L | k::Control_R => 0x11,
        k::Shift_L | k::Shift_R => 0x10,
        k::Alt_L | k::Alt_R => 0x12,
        _ => key
            .to_unicode()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase() as i32)
            .unwrap_or(0),
    }
}

pub struct ChromiumPage {
    area: gtk::DrawingArea,
    inner: Rc<Inner>,
    find_last: RefCell<String>,
    muted: std::cell::Cell<bool>,
}

impl ChromiumPage {
    pub fn new(uri: &str, zoom: f64, _accelerated: bool) -> Option<Box<dyn Page>> {
        if !ensure_initialized() {
            return None;
        }
        let inner = Rc::new(Inner::new(zoom));
        if !uri.is_empty() {
            inner.pending_uri.replace(Some(uri.to_string()));
        }

        let drain_weak = Rc::downgrade(&inner);
        glib::timeout_add_local(std::time::Duration::from_millis(15), move || match drain_weak.upgrade() {
            Some(inner) => {
                drain_bridge(&inner);
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });

        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_can_focus(true);
        area.add_events(
            gdk::EventMask::POINTER_MOTION_MASK
                | gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::SCROLL_MASK
                | gdk::EventMask::KEY_PRESS_MASK
                | gdk::EventMask::KEY_RELEASE_MASK
                | gdk::EventMask::ENTER_NOTIFY_MASK
                | gdk::EventMask::LEAVE_NOTIFY_MASK
                | gdk::EventMask::FOCUS_CHANGE_MASK,
        );
        inner.area.replace(Some(area.downgrade()));

        let draw_inner = inner.clone();
        area.connect_draw(move |_area, cr| {
            if let Some(surface) = draw_inner.surface.borrow().as_ref() {
                let _ = cr.set_source_surface(surface, 0.0, 0.0);
                let _ = cr.paint();
            } else {
                cr.set_source_rgb(1.0, 1.0, 1.0);
                let _ = cr.paint();
            }
            glib::Propagation::Stop
        });

        let map_inner = inner.clone();
        area.connect_map(move |_area| {
            map_inner.mapped.set(true);
            if let Some(host) = host_of(&map_inner) {
                host.was_hidden(0);
                host.was_resized();
            }
            try_load(&map_inner);
        });

        let unmap_inner = inner.clone();
        area.connect_unmap(move |_area| {
            unmap_inner.mapped.set(false);
            if let Some(host) = host_of(&unmap_inner) {
                host.was_hidden(1);
            }
        });

        let resize_inner = inner.clone();
        area.connect_size_allocate(move |_area, alloc| {
            let new = (alloc.width().max(1), alloc.height().max(1));
            if resize_inner.size.get() != new {
                resize_inner.size.set(new);
                if let Some(browser) = resize_inner.browser.borrow().as_ref() {
                    if let Some(host) = browser.host() {
                        host.was_resized();
                    }
                }
            }
        });

        let motion_inner = inner.clone();
        area.connect_motion_notify_event(move |_area, ev| {
            if let Some(host) = host_of(&motion_inner) {
                let (x, y) = ev.position();
                let event = MouseEvent {
                    x: x as i32,
                    y: y as i32,
                    modifiers: mods_from_state(ev.state()),
                };
                host.send_mouse_move_event(Some(&event), 0);
            }
            glib::Propagation::Proceed
        });

        let press_inner = inner.clone();
        area.connect_button_press_event(move |area, ev| {
            area.grab_focus();
            if let Some(host) = host_of(&press_inner) {
                let (x, y) = ev.position();
                let event = MouseEvent {
                    x: x as i32,
                    y: y as i32,
                    modifiers: mods_from_state(ev.state()),
                };
                let button = match ev.button() {
                    2 => MouseButtonType::MIDDLE,
                    3 => MouseButtonType::RIGHT,
                    _ => MouseButtonType::LEFT,
                };
                host.set_focus(1);
                host.send_mouse_click_event(Some(&event), button, 0, 1);
            }
            glib::Propagation::Proceed
        });

        let release_inner = inner.clone();
        area.connect_button_release_event(move |_area, ev| {
            if let Some(host) = host_of(&release_inner) {
                let (x, y) = ev.position();
                let event = MouseEvent {
                    x: x as i32,
                    y: y as i32,
                    modifiers: mods_from_state(ev.state()),
                };
                let button = match ev.button() {
                    2 => MouseButtonType::MIDDLE,
                    3 => MouseButtonType::RIGHT,
                    _ => MouseButtonType::LEFT,
                };
                host.send_mouse_click_event(Some(&event), button, 1, 1);
            }
            glib::Propagation::Proceed
        });

        let scroll_inner = inner.clone();
        let scroll_accum = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
        area.connect_scroll_event(move |_area, ev| {
            if let Some(host) = host_of(&scroll_inner) {
                let (x, y) = ev.position();
                let event = MouseEvent {
                    x: x as i32,
                    y: y as i32,
                    modifiers: mods_from_state(ev.state()),
                };
                let (mut dx, mut dy) = ev.delta();
                let mut step = 40.0;
                if dx == 0.0 && dy == 0.0 {
                    step = 100.0;
                    match ev.direction() {
                        gdk::ScrollDirection::Up => dy = -1.0,
                        gdk::ScrollDirection::Down => dy = 1.0,
                        gdk::ScrollDirection::Left => dx = -1.0,
                        gdk::ScrollDirection::Right => dx = 1.0,
                        _ => {}
                    }
                }
                let (ax, ay) = scroll_accum.get();
                let tx = ax - dx * step;
                let ty = ay - dy * step;
                let ix = tx.trunc();
                let iy = ty.trunc();
                scroll_accum.set((tx - ix, ty - iy));
                if ix != 0.0 || iy != 0.0 {
                    host.send_mouse_wheel_event(Some(&event), ix as i32, iy as i32);
                }
            }
            glib::Propagation::Stop
        });

        let leave_inner = inner.clone();
        area.connect_leave_notify_event(move |_area, ev| {
            if let Some(host) = host_of(&leave_inner) {
                let (x, y) = ev.position();
                let event = MouseEvent {
                    x: x as i32,
                    y: y as i32,
                    modifiers: mods_from_state(ev.state()),
                };
                host.send_mouse_move_event(Some(&event), 1);
            }
            glib::Propagation::Proceed
        });

        let key_press_inner = inner.clone();
        area.connect_key_press_event(move |_area, ev| {
            send_key(&key_press_inner, ev, false);
            glib::Propagation::Stop
        });

        let key_release_inner = inner.clone();
        area.connect_key_release_event(move |_area, ev| {
            send_key(&key_release_inner, ev, true);
            glib::Propagation::Stop
        });

        spawn_browser(&inner);
        Some(Box::new(ChromiumPage { area, inner, find_last: RefCell::new(String::new()), muted: std::cell::Cell::new(false) }))
    }

    fn with_browser<R>(&self, f: impl FnOnce(&Browser) -> R) -> Option<R> {
        let browser = self.inner.browser.borrow().clone();
        browser.as_ref().map(f)
    }
}

fn send_key(inner: &Rc<Inner>, ev: &gdk::EventKey, release: bool) {
    let Some(host) = host_of(&inner) else {
        return;
    };
    let keyval = ev.keyval();
    let modifiers = mods_from_state(ev.state());
    let vk = vk_from_keyval(&keyval);
    let native = ev.hardware_keycode() as i32;

    let mut event = KeyEvent {
        modifiers,
        windows_key_code: vk,
        native_key_code: native,
        ..Default::default()
    };

    if release {
        event.type_ = KeyEventType::KEYUP;
        host.send_key_event(Some(&event));
        return;
    }

    event.type_ = KeyEventType::RAWKEYDOWN;
    host.send_key_event(Some(&event));

    if let Some(ch) = keyval.to_unicode() {
        if !ch.is_control() {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units).iter() {
                let mut char_event = KeyEvent {
                    modifiers,
                    windows_key_code: *unit as i32,
                    native_key_code: native,
                    character: *unit,
                    unmodified_character: *unit,
                    ..Default::default()
                };
                char_event.type_ = KeyEventType::CHAR;
                host.send_key_event(Some(&char_event));
            }
        }
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
        self.inner.pending_uri.replace(Some(uri.to_string()));
        try_load(&self.inner);
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

    fn find(&self, text: &str, backwards: bool) {
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                if text.is_empty() {
                    host.stop_finding(1);
                    self.find_last.replace(String::new());
                } else {
                    let find_next = *self.find_last.borrow() == text;
                    host.find(
                        Some(&CefString::from(text)),
                        i32::from(!backwards),
                        0,
                        i32::from(find_next),
                    );
                    self.find_last.replace(text.to_string());
                }
            }
        });
    }

    fn is_muted(&self) -> bool {
        self.muted.get()
    }

    fn set_muted(&self, muted: bool) {
        self.muted.set(muted);
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                host.set_audio_muted(i32::from(muted));
            }
        });
    }

    fn set_request_logging(&self, on: bool) {
        self.inner.bridge.log_requests.store(on, Ordering::Relaxed);
    }

    fn set_intercepting(&self, on: bool) {
        self.inner.bridge.intercepting.store(on, Ordering::Relaxed);
    }

    fn set_intercept_route(&self, route: crate::engine::InterceptRoute) {
        self.inner.on_intercept.replace(Some(route));
    }

    fn close(&self) {
        self.with_browser(|browser| {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        });
    }
}
