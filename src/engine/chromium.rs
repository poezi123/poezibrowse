use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
            command_line.append_switch(Some(&CefString::from("disable-gpu")));
            command_line.append_switch(Some(&CefString::from("disable-gpu-compositing")));
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
    settings.windowless_rendering_enabled = 1;
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
            can_back: Cell::new(false),
            can_fwd: Cell::new(false),
            zoom: Cell::new(zoom),
            pending_uri: RefCell::new(None),
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
            _dirty_rects: Option<&[Rect]>,
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
            let mut data = vec![0u8; len];
            unsafe {
                std::ptr::copy_nonoverlapping(buffer, data.as_mut_ptr(), len);
            }
            if let Ok(surface) =
                cairo::ImageSurface::create_for_data(data, cairo::Format::ARgb32, width, height, stride)
            {
                self.inner.surface.replace(Some(surface));
                self.inner.queue_draw();
            }
        }
    }
}

wrap_client! {
    pub struct PoeziClient {
        life: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
        render: RenderHandler,
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
    }
}

fn spawn_browser(inner: &Rc<Inner>) {
    let life = PoeziLifeSpan::new(inner.clone());
    let load = PoeziLoad::new(inner.clone());
    let display = PoeziDisplay::new(inner.clone());
    let render = PoeziRender::new(inner.clone());
    let mut client = PoeziClient::new(life, load, display, render);

    let url = inner.pending_uri.borrow().clone();
    let url_cef = url.as_deref().map(CefString::from);
    let mut window_info = WindowInfo::default().set_as_windowless(0);
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
    if ok == 1 {
        inner.pending_uri.replace(None);
    }
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
            }
            glib::Propagation::Proceed
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
            if let Some(host) = motion_inner.browser.borrow().as_ref().and_then(Browser::host) {
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
            if let Some(host) = press_inner.browser.borrow().as_ref().and_then(Browser::host) {
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
            if let Some(host) = release_inner.browser.borrow().as_ref().and_then(Browser::host) {
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
        area.connect_scroll_event(move |_area, ev| {
            if let Some(host) = scroll_inner.browser.borrow().as_ref().and_then(Browser::host) {
                let (x, y) = ev.position();
                let event = MouseEvent {
                    x: x as i32,
                    y: y as i32,
                    modifiers: mods_from_state(ev.state()),
                };
                let (mut dx, mut dy) = ev.delta();
                if dx == 0.0 && dy == 0.0 {
                    match ev.direction() {
                        gdk::ScrollDirection::Up => dy = -1.0,
                        gdk::ScrollDirection::Down => dy = 1.0,
                        gdk::ScrollDirection::Left => dx = -1.0,
                        gdk::ScrollDirection::Right => dx = 1.0,
                        _ => {}
                    }
                }
                host.send_mouse_wheel_event(Some(&event), (-dx * 40.0) as i32, (-dy * 40.0) as i32);
            }
            glib::Propagation::Stop
        });

        let leave_inner = inner.clone();
        area.connect_leave_notify_event(move |_area, ev| {
            if let Some(host) = leave_inner.browser.borrow().as_ref().and_then(Browser::host) {
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
        Some(Box::new(ChromiumPage { area, inner }))
    }

    fn with_browser<R>(&self, f: impl FnOnce(&Browser) -> R) -> Option<R> {
        self.inner.browser.borrow().as_ref().map(f)
    }
}

fn send_key(inner: &Rc<Inner>, ev: &gdk::EventKey, release: bool) {
    let Some(host) = inner.browser.borrow().as_ref().and_then(Browser::host) else {
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
