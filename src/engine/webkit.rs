use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk_pixbuf::{InterpType, Pixbuf};
use gtk::glib;
use gtk::prelude::*;
use webkit2gtk::{
    AutoplayPolicy, CacheModel, HardwareAccelerationPolicy, LoadEvent, MemoryPressureSettings,
    NavigationPolicyDecision, PermissionRequest, PolicyDecision, PolicyDecisionType,
    ResponsePolicyDecision, Settings, SettingsExt, URISchemeRequest, URISchemeRequestExt,
    UserContentManager, WebContext, WebContextExt, WebView, WebViewExt, WebViewSessionState,
    WebsiteDataManager, WebsiteDataManagerExt, WebsitePolicies, FindOptions, FindControllerExt,
    PolicyDecisionExt, URIRequestExt, WebInspectorExt, HitTestResultExt, NavigationPolicyDecisionExt,
    ResponsePolicyDecisionExt, PrintOperationExt, SecurityManagerExt, BackForwardListExt,
    NetworkProxyMode, NetworkProxySettings, WebsiteDataManagerExtManual, WebsiteDataTypes,
    ContextMenuAction, ContextMenuExt, ContextMenuItem, ContextMenuItemExt,
};

use super::{EventSink, Page, PageEvent, SinkSlot};
use crate::blocker::{self, Blocker};
use crate::config::{self, Config, EngineKind, Mode};

pub type SchemeHandler = Rc<dyn Fn(&str) -> (String, Vec<u8>)>;

pub struct WebKitEngine {
    context: WebContext,
    settings: Settings,
    policies: WebsitePolicies,
    data_manager: WebsiteDataManager,
    tabs_per_process: u32,
    mode: Mode,
    process_groups: RefCell<Vec<(String, Vec<glib::WeakRef<WebView>>)>>,
    content_manager_hook: RefCell<Option<Rc<dyn Fn(&UserContentManager, &WebView)>>>,
    blocking: RefCell<Option<Blocking>>,
    youtube: crate::youtube::YoutubeBlocker,
    media_fix: webkit2gtk::UserScript,
    managers: RefCell<Vec<glib::WeakRef<UserContentManager>>>,
}

#[derive(Clone)]
pub struct Blocking {
    pub blocker: Rc<Blocker>,
    pub enabled_for: Rc<dyn Fn(&str) -> bool>,
}

impl WebKitEngine {
    pub fn new(config: &Config, mode: Mode, tor_port: u16, scheme_handler: SchemeHandler) -> Rc<Self> {
        let mut pressure = MemoryPressureSettings::new();
        if config.memory_limit_mb > 0 {
            pressure.set_memory_limit(config.memory_limit_mb);
        }
        pressure.set_conservative_threshold(0.33);
        pressure.set_strict_threshold(0.5);

        let data_manager = if mode.is_private() {
            WebsiteDataManager::new_ephemeral()
        } else {
            let data = config::data_dir().join("webkit");
            let cache = config::cache_dir().join("webkit");
            WebsiteDataManager::builder()
                .base_data_directory(data.to_string_lossy().to_string())
                .base_cache_directory(cache.to_string_lossy().to_string())
                .build()
        };
        data_manager.set_itp_enabled(true);
        WebsiteDataManager::set_memory_pressure_settings(&mut pressure.clone());
        if mode == Mode::Tor {
            let proxy = format!("socks5://127.0.0.1:{tor_port}");
            let mut proxy_settings = NetworkProxySettings::new(Some(&proxy), &[]);
            data_manager.set_network_proxy_settings(NetworkProxyMode::Custom, Some(&mut proxy_settings));
        }

        let mut builder = WebContext::builder()
            .website_data_manager(&data_manager)
            .memory_pressure_settings(&pressure);
        if mode == Mode::Tor {
            builder = builder.time_zone_override("Etc/UTC");
        }
        let context = builder.build();
        if mode == Mode::Tor {
            context.set_preferred_languages(&["en-US", "en"]);
        }
        context.set_cache_model(if config.saver_mode {
            CacheModel::DocumentBrowser
        } else {
            CacheModel::WebBrowser
        });
        if !mode.is_private() {
            let favicons = config::cache_dir().join("favicons");
            context.set_favicon_database_directory(Some(&favicons.to_string_lossy()));
        }
        if sandbox_usable() {
            context.set_sandbox_enabled(true);
        }

        let security = context.security_manager().expect("WebContext without security manager");
        security.register_uri_scheme_as_local("poezi");
        security.register_uri_scheme_as_secure("poezi");
        context.register_uri_scheme("poezi", move |request: &URISchemeRequest| {
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            let (mime, body) = scheme_handler(&uri);
            let len = body.len() as i64;
            let stream = gtk::gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(body));
            request.finish(&stream, len, Some(&mime));
        });

        let settings = Settings::new();
        settings.set_enable_developer_extras(true);
        settings.set_enable_smooth_scrolling(true);
        settings.set_enable_back_forward_navigation_gestures(true);
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_enable_page_cache(true);
        settings.set_hardware_acceleration_policy(if config.hardware_acceleration {
            HardwareAccelerationPolicy::Always
        } else {
            HardwareAccelerationPolicy::Never
        });
        enable_feature(&settings, "PDFJSViewerEnabled");
        if mode == Mode::Tor {
            settings.set_enable_webrtc(false);
            settings.set_enable_media_stream(false);
            settings.set_enable_webgl(false);
            settings.set_enable_encrypted_media(false);
            settings.set_enable_javascript(config.tor_javascript);
        }

        let policies = WebsitePolicies::builder()
            .autoplay(if config.autoplay_sound {
                AutoplayPolicy::Allow
            } else {
                AutoplayPolicy::AllowWithoutSound
            })
            .build();

        Rc::new(WebKitEngine {
            context,
            settings,
            policies,
            data_manager,
            tabs_per_process: config.tabs_per_process.max(1),
            mode,
            process_groups: RefCell::new(Vec::new()),
            content_manager_hook: RefCell::new(None),
            blocking: RefCell::new(None),
            youtube: crate::youtube::YoutubeBlocker::new(),
            media_fix: webkit2gtk::UserScript::new(
                include_str!("media_fix.js"),
                webkit2gtk::UserContentInjectedFrames::AllFrames,
                webkit2gtk::UserScriptInjectionTime::Start,
                &[],
                &[],
            ),
            managers: RefCell::new(Vec::new()),
        })
    }

    pub fn context(&self) -> &WebContext {
        &self.context
    }

    pub fn release_memory(&self) {
        self.data_manager.clear(WebsiteDataTypes::MEMORY_CACHE, glib::TimeSpan(0), None::<&gtk::gio::Cancellable>, |_| {});
    }

    pub fn favicon_database(&self) -> Option<webkit2gtk::FaviconDatabase> {
        if self.mode.is_private() { None } else { self.context.favicon_database() }
    }

    pub fn set_content_manager_hook(&self, hook: Rc<dyn Fn(&UserContentManager, &WebView)>) {
        *self.content_manager_hook.borrow_mut() = Some(hook);
    }

    fn process_partner(&self, site: &str) -> (Option<WebView>, usize) {
        let mut groups = self.process_groups.borrow_mut();
        for group in groups.iter_mut() {
            group.1.retain(|w| w.upgrade().is_some());
        }
        groups.retain(|g| !g.1.is_empty());
        let limit = self.tabs_per_process as usize;
        if limit > 1 && !site.is_empty() {
            for (i, group) in groups.iter().enumerate() {
                if group.0 == site && group.1.len() < limit {
                    return (group.1[0].upgrade(), i);
                }
            }
        }
        groups.push((site.to_string(), Vec::new()));
        (None, groups.len() - 1)
    }

    fn register_in_group(&self, view: &WebView, group: usize) {
        let mut groups = self.process_groups.borrow_mut();
        if let Some(g) = groups.get_mut(group) {
            g.1.push(view.downgrade());
        }
    }

    fn register_with(&self, view: &WebView, partner: &WebView) {
        let mut groups = self.process_groups.borrow_mut();
        let partner_ptr = partner.as_ptr();
        if let Some(g) = groups
            .iter_mut()
            .find(|g| g.1.iter().any(|w| w.upgrade().is_some_and(|v| v.as_ptr() == partner_ptr)))
        {
            g.1.push(view.downgrade());
        } else {
            groups.push((String::new(), vec![view.downgrade()]));
        }
    }

    pub fn set_blocking(self: &Rc<Self>, blocking: Blocking) {
        let me = Rc::downgrade(self);
        blocking.blocker.connect_ready(move |filters| {
            let Some(me) = me.upgrade() else { return };
            let mut managers = me.managers.borrow_mut();
            managers.retain(|w| w.upgrade().is_some());
            for ucm in managers.iter().filter_map(|w| w.upgrade()) {
                blocker::remove_filters(&ucm);
                for filter in filters {
                    blocker::add_filter(&ucm, filter);
                }
            }
        });
        *self.blocking.borrow_mut() = Some(blocking);
    }

    fn update_blocking(&self, ucm: &UserContentManager, uri: &str) {
        let Some(blocking) = self.blocking.borrow().clone() else { return };
        let host = config::host_of(uri).unwrap_or_default();
        blocker::remove_filters(ucm);
        let enabled = (blocking.enabled_for)(&host);
        self.youtube.apply(ucm, enabled);
        if enabled {
            for filter in blocking.blocker.filters.borrow().iter() {
                blocker::add_filter(ucm, filter);
            }
        }
    }

    fn new_content_manager(&self) -> UserContentManager {
        let ucm = UserContentManager::new();
        webkit2gtk::UserContentManagerExt::add_script(&ucm, &self.media_fix);
        if let Some(blocking) = self.blocking.borrow().as_ref() {
            for filter in blocking.blocker.filters.borrow().iter() {
                blocker::add_filter(&ucm, filter);
            }
            self.youtube.apply(&ucm, true);
        }
        self.managers.borrow_mut().push(ucm.downgrade());
        ucm
    }

    pub fn create_page(self: &Rc<Self>, related: Option<&WebView>, uri: &str) -> Rc<WebKitPage> {
        let site = site_key(uri);
        let ucm = self.new_content_manager();
        let builder = WebView::builder()
            .user_content_manager(&ucm)
            .settings(&self.settings)
            .website_policies(&self.policies);
        let view = match related {
            Some(opener) => {
                let view = builder.related_view(opener).build();
                self.register_with(&view, opener);
                view
            }
            None => {
                let (partner, group) = self.process_partner(&site);
                let view = match &partner {
                    Some(p) => builder.related_view(p).build(),
                    None => builder.web_context(&self.context).build(),
                };
                self.register_in_group(&view, group);
                view
            }
        };
        if let Some(hook) = self.content_manager_hook.borrow().clone() {
            hook(&ucm, &view);
        }
        WebKitPage::new(self.clone(), view, ucm)
    }
}

pub struct WebKitPage {
    engine: Rc<WebKitEngine>,
    view: WebView,
    content_manager: UserContentManager,
    sink: SinkSlot,
}

impl WebKitPage {
    fn new(engine: Rc<WebKitEngine>, view: WebView, content_manager: UserContentManager) -> Rc<Self> {
        let page = Rc::new(WebKitPage { engine, view, content_manager, sink: SinkSlot::default() });
        page.connect_signals();
        page.view.show();
        page
    }

    pub fn restore_state(&self, data: &[u8]) -> bool {
        let state = WebViewSessionState::new(&glib::Bytes::from(data));
        self.view.restore_session_state(&state);
        let current = self.view.back_forward_list().and_then(|l| l.current_item());
        match current {
            Some(item) => {
                self.view.go_to_back_forward_list_item(&item);
                true
            }
            None => false,
        }
    }

    fn connect_signals(self: &Rc<Self>) {
        let v = &self.view;
        let sink = self.sink.clone();

        let s = sink.clone();
        v.connect_title_notify(move |_| s.emit(PageEvent::TitleChanged));
        let s = sink.clone();
        v.connect_uri_notify(move |_| {
            s.emit(PageEvent::UriChanged);
            s.emit(PageEvent::NavigationStateChanged);
        });
        let s = sink.clone();
        v.connect_estimated_load_progress_notify(move |v| s.emit(PageEvent::Progress(v.estimated_load_progress())));
        let s = sink.clone();
        v.connect_is_loading_notify(move |_| s.emit(PageEvent::LoadingChanged));
        let s = sink.clone();
        let engine = Rc::downgrade(&self.engine);
        let ucm = self.content_manager.clone();
        v.connect_load_changed(move |view, event| {
            if matches!(event, LoadEvent::Started | LoadEvent::Redirected) {
                if let (Some(engine), Some(uri)) = (engine.upgrade(), view.uri()) {
                    engine.update_blocking(&ucm, &uri);
                }
            }
            s.emit(PageEvent::NavigationStateChanged);
            if event == LoadEvent::Finished {
                s.emit(PageEvent::LoadFinished);
            }
        });
        let s = sink.clone();
        v.connect_favicon_notify(move |_| s.emit(PageEvent::FaviconChanged));
        let s = sink.clone();
        v.connect_is_playing_audio_notify(move |_| s.emit(PageEvent::AudioChanged));
        let s = sink.clone();
        v.connect_mouse_target_changed(move |_, hit, _| {
            let link = hit.context_is_link().then(|| hit.link_uri()).flatten().map(|u| u.to_string());
            s.emit(PageEvent::HoverLink(link));
        });
        let s = sink.clone();
        v.connect_enter_fullscreen(move |_| {
            s.emit(PageEvent::Fullscreen(true));
            false
        });
        let s = sink.clone();
        v.connect_leave_fullscreen(move |_| {
            s.emit(PageEvent::Fullscreen(false));
            false
        });
        let s = sink.clone();
        v.connect_web_process_terminated(move |_, reason| {
            let text = match reason {
                webkit2gtk::WebProcessTerminationReason::ExceededMemoryLimit => "zu viel Arbeitsspeicher",
                webkit2gtk::WebProcessTerminationReason::TerminatedByApi => "beendet",
                _ => "Absturz",
            };
            s.emit(PageEvent::Crashed(text.to_string()));
        });
        let s = sink.clone();
        v.connect_close(move |_| s.emit(PageEvent::CloseRequested));

        let engine = self.engine.clone();
        let s = sink.clone();
        v.connect_create(move |opener, _action| {
            let page = engine.create_page(Some(opener), "");
            let widget = page.view.clone().upcast::<gtk::Widget>();
            s.emit(PageEvent::Popup(Box::new(PageHandle(page))));
            Some(widget)
        });

        let s = sink.clone();
        v.connect_decide_policy(move |_, decision, kind| decide_policy(&s, decision, kind));

        v.connect_button_press_event(|view, event| match event.button() {
            8 => {
                view.go_back();
                glib::Propagation::Stop
            }
            9 => {
                view.go_forward();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });

        v.connect_scroll_event(|view, event| {
            if !event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            let delta = match event.direction() {
                gtk::gdk::ScrollDirection::Up => -1.0,
                gtk::gdk::ScrollDirection::Down => 1.0,
                gtk::gdk::ScrollDirection::Smooth => event.delta().1,
                _ => 0.0,
            };
            if delta != 0.0 {
                let step = if delta < 0.0 { 0.1 } else { -0.1 };
                view.set_zoom_level((view.zoom_level() + step).clamp(0.3, 5.0));
            }
            glib::Propagation::Stop
        });

        let s = sink.clone();
        v.connect_context_menu(move |_, menu, _, hit| {
            let new_tab_item = |label: &str, uri: Option<glib::GString>, background: bool| {
                let uri = uri?.to_string();
                let action = gtk::gio::SimpleAction::new("open-in-tab", None);
                let s = s.clone();
                action.connect_activate(move |_, _| s.emit(PageEvent::OpenInNewTab(uri.clone(), background)));
                Some(ContextMenuItem::from_gaction(&action, label, None))
            };
            for (position, item) in menu.items().into_iter().enumerate() {
                let replacement = match item.stock_action() {
                    ContextMenuAction::OpenLinkInNewWindow => new_tab_item("Link in neuem Tab öffnen", hit.link_uri(), true),
                    ContextMenuAction::OpenImageInNewWindow => new_tab_item("Bild in neuem Tab öffnen", hit.image_uri(), false),
                    ContextMenuAction::OpenVideoInNewWindow | ContextMenuAction::OpenAudioInNewWindow => {
                        new_tab_item("Medium in neuem Tab öffnen", hit.media_uri(), false)
                    }
                    ContextMenuAction::OpenFrameInNewWindow => {
                        menu.remove(&item);
                        continue;
                    }
                    _ => continue,
                };
                if let Some(new_item) = replacement {
                    menu.remove(&item);
                    menu.insert(&new_item, position as i32);
                }
            }
            false
        });

        v.connect_permission_request(|view, request| handle_permission(view, request));

        v.connect_load_failed_with_tls_errors(|view, uri, _cert, _flags| {
            let host = config::host_of(uri).unwrap_or_default();
            let html = crate::pages::error_page(
                "Unsichere Verbindung",
                &format!("Das Zertifikat von <b>{}</b> ist ungültig. PoeziBrowse hat die Seite deshalb nicht geladen.", crate::pages::escape(&host)),
            );
            view.load_alternate_html(&html, uri, None);
            true
        });
    }
}

fn decide_policy(sink: &SinkSlot, decision: &PolicyDecision, kind: PolicyDecisionType) -> bool {
    match kind {
        PolicyDecisionType::NavigationAction => {
            let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else { return false };
            let Some(action) = nav.navigation_action() else { return false };
            let ctrl = action.modifiers() & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;
            let middle = action.mouse_button() == 2;
            if middle || ctrl {
                if let Some(uri) = action.request().and_then(|r| r.uri()) {
                    decision.ignore();
                    sink.emit(PageEvent::OpenInNewTab(uri.to_string(), true));
                    return true;
                }
            }
            false
        }
        PolicyDecisionType::Response => {
            let Some(resp) = decision.downcast_ref::<ResponsePolicyDecision>() else { return false };
            if !resp.is_mime_type_supported() {
                decision.download();
                return true;
            }
            false
        }
        _ => false,
    }
}

fn handle_permission(view: &WebView, request: &PermissionRequest) -> bool {
    use webkit2gtk::{PermissionRequestExt, PointerLockPermissionRequest, UserMediaPermissionRequest};
    if request.is::<PointerLockPermissionRequest>() {
        request.allow();
        return true;
    }
    if let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() {
        use webkit2gtk::UserMediaPermissionRequestExt;
        let what = match (media.is_for_audio_device(), media.is_for_video_device()) {
            (true, true) => "Kamera und Mikrofon",
            (false, true) => "die Kamera",
            _ => "das Mikrofon",
        };
        let host = view.uri().and_then(|u| config::host_of(&u)).unwrap_or_default();
        let parent = view.toplevel().and_then(|w| w.downcast::<gtk::Window>().ok());
        let dialog = gtk::MessageDialog::new(
            parent.as_ref(),
            gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
            gtk::MessageType::Question,
            gtk::ButtonsType::None,
            &format!("{host} möchte auf {what} zugreifen."),
        );
        dialog.add_buttons(&[("Ablehnen", gtk::ResponseType::Reject), ("Erlauben", gtk::ResponseType::Accept)]);
        let request = request.clone();
        dialog.connect_response(move |d, response| {
            if response == gtk::ResponseType::Accept {
                request.allow();
            } else {
                request.deny();
            }
            d.close();
        });
        dialog.show_all();
        return true;
    }
    request.deny();
    true
}

fn enable_feature(settings: &Settings, identifier: &str) -> bool {
    type AllFeatures = unsafe extern "C" fn() -> *mut std::ffi::c_void;
    type Length = unsafe extern "C" fn(*mut std::ffi::c_void) -> usize;
    type Get = unsafe extern "C" fn(*mut std::ffi::c_void, usize) -> *mut std::ffi::c_void;
    type Identifier = unsafe extern "C" fn(*mut std::ffi::c_void) -> *const std::ffi::c_char;
    type SetEnabled = unsafe extern "C" fn(*mut webkit2gtk::ffi::WebKitSettings, *mut std::ffi::c_void, i32);
    type Unref = unsafe extern "C" fn(*mut std::ffi::c_void);
    use crate::system::symbol;
    use gtk::glib::translate::ToGlibPtr;
    let (Some(all), Some(length), Some(get), Some(id), Some(set), Some(unref)) = (
        symbol("webkit_settings_get_all_features"),
        symbol("webkit_feature_list_get_length"),
        symbol("webkit_feature_list_get"),
        symbol("webkit_feature_get_identifier"),
        symbol("webkit_settings_set_feature_enabled"),
        symbol("webkit_feature_list_unref"),
    ) else {
        return false;
    };
    unsafe {
        let all: AllFeatures = std::mem::transmute(all);
        let length: Length = std::mem::transmute(length);
        let get: Get = std::mem::transmute(get);
        let id: Identifier = std::mem::transmute(id);
        let set: SetEnabled = std::mem::transmute(set);
        let unref: Unref = std::mem::transmute(unref);
        let list = all();
        if list.is_null() {
            return false;
        }
        let mut found = false;
        for i in 0..length(list) {
            let feature = get(list, i);
            let name = id(feature);
            if !name.is_null() && std::ffi::CStr::from_ptr(name).to_bytes() == identifier.as_bytes() {
                set(settings.to_glib_none().0, feature, 1);
                found = true;
                break;
            }
        }
        unref(list);
        found
    }
}

pub fn site_key(uri: &str) -> String {
    if uri.starts_with("poezi:") {
        return "poezi".into();
    }
    let Some(host) = config::host_of(uri) else { return String::new() };
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    let n = labels.len();
    if n <= 2 {
        return labels.join(".");
    }
    let second = labels[n - 2];
    let take = if labels[n - 1].len() == 2 && second.len() <= 3 && ["co", "com", "org", "net", "ac", "gov", "edu", "or", "ne"].contains(&second) { 3 } else { 2 };
    labels[n - take..].join(".")
}

pub fn sandbox_usable() -> bool {
    static USABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USABLE.get_or_init(|| {
        which("bwrap")
            && std::process::Command::new("bwrap")
                .args(["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--unshare-all", "true"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
    })
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(program).is_file()))
        .unwrap_or(false)
}

pub struct PageHandle(pub Rc<WebKitPage>);

impl Page for PageHandle {
    fn engine(&self) -> EngineKind {
        EngineKind::Webkit
    }
    fn widget(&self) -> gtk::Widget {
        self.0.view.clone().upcast()
    }
    fn set_event_sink(&self, sink: EventSink) {
        self.0.sink.set(sink);
    }
    fn load_uri(&self, uri: &str) {
        self.0.view.load_uri(uri);
    }
    fn reload(&self, bypass_cache: bool) {
        if bypass_cache {
            self.0.view.reload_bypass_cache();
        } else {
            self.0.view.reload();
        }
    }
    fn stop(&self) {
        self.0.view.stop_loading();
    }
    fn go_back(&self) {
        self.0.view.go_back();
    }
    fn go_forward(&self) {
        self.0.view.go_forward();
    }
    fn uri(&self) -> Option<String> {
        self.0.view.uri().map(|u| u.to_string())
    }
    fn title(&self) -> Option<String> {
        self.0.view.title().map(|t| t.to_string()).filter(|t| !t.is_empty())
    }
    fn favicon(&self) -> Option<Pixbuf> {
        let surface = self.0.view.favicon()?;
        let image = gtk::cairo::ImageSurface::try_from(surface).ok()?;
        let (w, h) = (image.width(), image.height());
        let pixbuf = gtk::gdk::pixbuf_get_from_surface(&image, 0, 0, w, h)?;
        let size = 16 * self.0.view.scale_factor();
        pixbuf.scale_simple(size, size, InterpType::Bilinear)
    }
    fn can_go_back(&self) -> bool {
        self.0.view.can_go_back()
    }
    fn can_go_forward(&self) -> bool {
        self.0.view.can_go_forward()
    }
    fn is_loading(&self) -> bool {
        self.0.view.is_loading()
    }
    fn zoom(&self) -> f64 {
        self.0.view.zoom_level()
    }
    fn set_zoom(&self, level: f64) {
        self.0.view.set_zoom_level(level);
    }
    fn is_playing_audio(&self) -> bool {
        self.0.view.is_playing_audio()
    }
    fn is_muted(&self) -> bool {
        self.0.view.is_muted()
    }
    fn set_muted(&self, muted: bool) {
        self.0.view.set_is_muted(muted);
    }
    fn find(&self, text: &str, backwards: bool) {
        let Some(finder) = self.0.view.find_controller() else { return };
        if text.is_empty() {
            finder.search_finish();
            return;
        }
        if finder.search_text().is_some_and(|t| t == text) {
            if backwards {
                finder.search_previous();
            } else {
                finder.search_next();
            }
        } else {
            let mut options = FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND;
            if backwards {
                options |= FindOptions::BACKWARDS;
            }
            finder.search(text, options.bits(), u32::MAX);
        }
    }
    fn print(&self, parent: &gtk::Window) {
        let op = webkit2gtk::PrintOperation::new(&self.0.view);
        op.run_dialog(Some(parent));
    }
    fn show_inspector(&self) {
        if let Some(inspector) = self.0.view.inspector() {
            inspector.show();
        }
    }
    fn webview(&self) -> Option<WebView> {
        Some(self.0.view.clone())
    }
    fn save_state(&self) -> Option<Vec<u8>> {
        let state = self.0.view.session_state()?;
        Some(state.serialize()?.to_vec())
    }
    fn restore_state(&self, state: &[u8]) -> bool {
        self.0.restore_state(state)
    }
    fn close(&self) {
        self.0.view.stop_loading();
    }
}

#[cfg(test)]
mod tests {
    use super::site_key;

    #[test]
    fn sites() {
        assert_eq!(site_key("https://www.youtube.com/watch?v=1"), "youtube.com");
        assert_eq!(site_key("https://m.youtube.com/"), "youtube.com");
        assert_eq!(site_key("https://www.amazon.co.uk/x"), "amazon.co.uk");
        assert_eq!(site_key("https://de.wikipedia.org/wiki/Rust"), "wikipedia.org");
        assert_eq!(site_key("poezi://start"), "poezi");
        assert_eq!(site_key("http://192.168.0.1/"), "192.168.0.1");
        assert_eq!(site_key("about:blank"), "");
    }
}
