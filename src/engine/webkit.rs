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
use crate::config::{self, Config, EngineKind, Fingerprint, Mode, PermissionKind, ProxyMode};

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
    tor_port: u16,
    fingerprint_script: Option<webkit2gtk::UserScript>,
    pub log_requests: std::cell::Cell<bool>,
    pub intercepting: std::cell::Cell<bool>,
    pub on_intercept: RefCell<Option<Rc<dyn Fn(Held)>>>,
    intercept_bypass: Rc<RefCell<std::collections::HashSet<String>>>,
    tor_https_only: bool,
    https_only: bool,
    https_exceptions: Rc<[String]>,
    block_local_net: bool,
    prewarm: bool,
    spare: RefCell<Option<(WebView, UserContentManager)>>,
    cookies: Option<Rc<crate::cookies::Keeper>>,
    pub permission_store: RefCell<Option<PermissionStore>>,
    pub certificate_allowed: RefCell<Option<Rc<dyn Fn(&str, &str) -> bool>>>,
    pub on_certificate_accepted: RefCell<Option<Rc<dyn Fn(&str, &str)>>>,
    pending_certificates: RefCell<Vec<(String, String, String, String)>>,
    applied: RefCell<std::collections::HashMap<usize, (Vec<&'static str>, bool, u64)>>,
}

#[derive(Clone)]
pub struct Blocking {
    pub blocker: Rc<Blocker>,
    pub enabled_for: Rc<dyn Fn(&str) -> bool>,
    pub filters_for: Rc<dyn Fn(&str) -> Vec<&'static str>>,
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
            set_tor_proxy(&data_manager, tor_port);
        } else {
            apply_proxy(&data_manager, config);
        }
        #[allow(deprecated)]
        let cookie_manager = data_manager.cookie_manager();
        let cookies = cookie_manager.and_then(|manager| {
            use webkit2gtk::{CookieAcceptPolicy, CookieManagerExt};
            manager.set_accept_policy(match (mode, config.cookie_policy) {
                (Mode::Tor, _) => CookieAcceptPolicy::NoThirdParty,
                (_, config::CookiePolicy::All) => CookieAcceptPolicy::Always,
                (_, config::CookiePolicy::NoThirdParty) => CookieAcceptPolicy::NoThirdParty,
                (_, config::CookiePolicy::None) => CookieAcceptPolicy::Never,
            });
            (mode == Mode::Normal && config.remember_logins && crate::cookies::supported()).then(|| crate::cookies::Keeper::new(manager))
        });
        let fingerprint = config.fingerprint_for(mode);
        let strict = fingerprint == Fingerprint::Strict;

        let mut builder = WebContext::builder()
            .website_data_manager(&data_manager)
            .memory_pressure_settings(&pressure);
        if strict {
            builder = builder.time_zone_override("Etc/UTC");
        }
        let context = builder.build();
        if strict {
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
        set_feature(&settings, "PDFJSViewerEnabled", true);
        let user_agent = config.user_agent_for(mode);
        if let Some((ua, _)) = &user_agent {
            settings.set_user_agent(Some(ua));
        }
        if fingerprint != Fingerprint::Off {
            set_feature(&settings, "GlobalPrivacyControl", true);
        }
        if strict {
            settings.set_enable_webrtc(false);
            settings.set_enable_media_stream(false);
            settings.set_enable_webgl(false);
            for feature in ["Gamepads", "SpeechSynthesisAPI", "OffscreenCanvasInWorkers", "WebGL"] {
                set_feature(&settings, feature, false);
            }
        } else if mode != Mode::Tor && !config.webrtc {
            settings.set_enable_webrtc(false);
        }
        if mode == Mode::Tor {
            settings.set_enable_encrypted_media(false);
            settings.set_enable_javascript(config.tor_javascript);
        }
        let fingerprint_script = (fingerprint != Fingerprint::Off).then(|| {
            let mut seed = [0u8; 4];
            let _ = getrandom::fill(&mut seed);
            let platform = user_agent.as_ref().map(|(_, p)| *p).unwrap_or("");
            let source = include_str!("fingerprint.js")
                .replace("__SEED__", &u32::from_le_bytes(seed).to_string())
                .replace("__PLATFORM__", &format!("'{platform}'"));
            webkit2gtk::UserScript::new(
                &source,
                webkit2gtk::UserContentInjectedFrames::AllFrames,
                webkit2gtk::UserScriptInjectionTime::Start,
                &[],
                &[],
            )
        });

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
                &include_str!("media_fix.js").replace("__BLOCK_AV1__", if crate::system::fast_av1() { "false" } else { "true" }),
                webkit2gtk::UserContentInjectedFrames::AllFrames,
                webkit2gtk::UserScriptInjectionTime::Start,
                &[],
                &[],
            ),
            managers: RefCell::new(Vec::new()),
            tor_port,
            fingerprint_script,
            log_requests: std::cell::Cell::new(false),
            intercepting: std::cell::Cell::new(false),
            on_intercept: RefCell::new(None),
            intercept_bypass: Rc::new(RefCell::new(std::collections::HashSet::new())),
            tor_https_only: config.tor_https_only,
            https_only: if mode == Mode::Tor { config.tor_https_only } else { config.https_only },
            https_exceptions: config.https_exceptions.clone().into(),
            block_local_net: mode != Mode::Tor && config.local_network_protection,
            prewarm: config.prewarm && mode == Mode::Normal,
            spare: RefCell::new(None),
            cookies,
            permission_store: RefCell::new(None),
            certificate_allowed: RefCell::new(None),
            on_certificate_accepted: RefCell::new(None),
            pending_certificates: RefCell::new(Vec::new()),
            applied: RefCell::new(std::collections::HashMap::new()),
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

    pub fn apply_proxy(&self, config: &Config) {
        if self.mode != Mode::Tor {
            apply_proxy(&self.data_manager, config);
        }
    }

    pub fn allow_certificate(&self, host: &str, pem: &str) {
        if self.mode == Mode::Tor {
            return;
        }
        if let Ok(cert) = gtk::gio::TlsCertificate::from_pem(pem) {
            self.context.allow_tls_certificate_for_host(&cert, host);
        }
    }

    fn certificate_error(&self, view: &WebView, uri: &str, cert: &gtk::gio::TlsCertificate, flags: gtk::gio::TlsCertificateFlags) {
        let host = config::host_of(uri).unwrap_or_default();
        let pem = cert.certificate_pem().map(|p| p.to_string()).unwrap_or_default();
        let nonce = if self.mode == Mode::Tor || host.is_empty() || pem.is_empty() {
            None
        } else {
            let mut bytes = [0u8; 16];
            let _ = getrandom::fill(&mut bytes);
            let nonce: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let mut pending = self.pending_certificates.borrow_mut();
            pending.retain(|p| p.3 != uri);
            pending.push((nonce.clone(), host, pem, uri.to_string()));
            if pending.len() > 20 {
                pending.remove(0);
            }
            Some(nonce)
        };
        show_certificate_error(view, uri, flags, nonce.as_deref());
    }

    fn accept_certificate(&self, nonce: &str) -> Option<String> {
        let (_, host, pem, uri) = {
            let mut pending = self.pending_certificates.borrow_mut();
            let i = pending.iter().position(|p| p.0 == nonce)?;
            pending.remove(i)
        };
        let callback = self.on_certificate_accepted.borrow().clone();
        match callback {
            Some(f) => f(&host, &pem),
            None => self.allow_certificate(&host, &pem),
        }
        Some(uri)
    }

    fn certificate_allowed(&self, host: &str, pem: &str) -> bool {
        self.mode != Mode::Tor && self.certificate_allowed.borrow().as_ref().is_some_and(|f| f(host, pem))
    }

    pub fn new_tor_circuits(&self) {
        if self.mode == Mode::Tor {
            set_tor_proxy(&self.data_manager, self.tor_port);
        }
    }

    pub fn set_blocking(self: &Rc<Self>, blocking: Blocking) {
        let me = Rc::downgrade(self);
        let blocker = Rc::downgrade(&blocking.blocker);
        let filters_for = blocking.filters_for.clone();
        blocking.blocker.connect_ready(move || {
            let (Some(me), Some(blocker)) = (me.upgrade(), blocker.upgrade()) else { return };
            let filters = blocker.select(&filters_for(""));
            let mut managers = me.managers.borrow_mut();
            managers.retain(|w| w.upgrade().is_some());
            for ucm in managers.iter().filter_map(|w| w.upgrade()) {
                blocker::remove_filters(&ucm);
                for filter in &filters {
                    blocker::add_filter(&ucm, filter);
                }
            }
        });
        *self.blocking.borrow_mut() = Some(blocking);
    }

    pub fn is_dangerous(&self, uri: &str) -> bool {
        let Some(blocking) = self.blocking.borrow().clone() else { return false };
        let Some(host) = config::host_of(uri) else { return false };
        (blocking.filters_for)(&host).contains(&blocker::MALWARE) && blocking.blocker.is_dangerous(&host)
    }

    fn update_blocking(&self, ucm: &UserContentManager, uri: &str) {
        let Some(blocking) = self.blocking.borrow().clone() else { return };
        let host = config::host_of(uri).unwrap_or_default();
        let ids = (blocking.filters_for)(&host);
        let youtube = (blocking.enabled_for)(&host);
        let generation = blocking.blocker.generation();
        let key = ucm.as_ptr() as usize;
        let wanted = (ids.clone(), youtube, generation);
        if self.applied.borrow().get(&key) == Some(&wanted) {
            return;
        }
        self.applied.borrow_mut().retain(|k, _| self.managers.borrow().iter().any(|w| w.upgrade().is_some_and(|u| u.as_ptr() as usize == *k)));
        self.applied.borrow_mut().insert(key, wanted);
        blocker::remove_filters(ucm);
        self.youtube.apply(ucm, youtube);
        for filter in blocking.blocker.select(&ids) {
            blocker::add_filter(ucm, &filter);
        }
    }

    fn new_content_manager(&self) -> UserContentManager {
        let ucm = UserContentManager::new();
        webkit2gtk::UserContentManagerExt::add_script(&ucm, &self.media_fix);
        if let Some(script) = &self.fingerprint_script {
            webkit2gtk::UserContentManagerExt::add_script(&ucm, script);
        }
        if let Some(blocking) = self.blocking.borrow().as_ref() {
            for filter in blocking.blocker.select(&(blocking.filters_for)("")) {
                blocker::add_filter(&ucm, &filter);
            }
            self.youtube.apply(&ucm, (blocking.enabled_for)(""));
        }
        self.managers.borrow_mut().push(ucm.downgrade());
        ucm
    }

    pub fn save_cookies(&self) {
        if let Some(keeper) = &self.cookies {
            keeper.save_now();
        }
    }

    pub fn prewarm_soon(self: &Rc<Self>) {
        if !self.prewarm {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            let Some(me) = me.upgrade() else { return };
            if me.spare.borrow().is_some() {
                return;
            }
            let ucm = me.new_content_manager();
            let view = WebView::builder()
                .user_content_manager(&ucm)
                .settings(&me.settings)
                .website_policies(&me.policies)
                .web_context(&me.context)
                .build();
            view.load_uri("about:blank");
            *me.spare.borrow_mut() = Some((view, ucm));
        });
    }

    pub fn create_page(self: &Rc<Self>, related: Option<&WebView>, uri: &str) -> Rc<WebKitPage> {
        let site = site_key(uri);
        if related.is_none() {
            let (partner, group) = self.process_partner(&site);
            if partner.is_none() {
                let spare = self.spare.borrow_mut().take();
                if let Some((spare, _)) = spare {
                    let ucm = self.new_content_manager();
                    let view = WebView::builder()
                        .user_content_manager(&ucm)
                        .settings(&self.settings)
                        .website_policies(&self.policies)
                        .related_view(&spare)
                        .build();
                    drop(spare);
                    self.register_in_group(&view, group);
                    if let Some(hook) = self.content_manager_hook.borrow().clone() {
                        hook(&ucm, &view);
                    }
                    self.prewarm_soon();
                    return WebKitPage::new(self.clone(), view, ucm);
                }
            }
            self.process_groups.borrow_mut().retain(|g| !g.1.is_empty());
        }
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
        let started: std::cell::Cell<Option<std::time::Instant>> = std::cell::Cell::new(None);
        let created = std::cell::Cell::new(Some(std::time::Instant::now()));
        v.connect_load_changed(move |view, event| {
            if event == LoadEvent::Started {
                started.set(Some(std::time::Instant::now()));
            }
            if event == LoadEvent::Finished && std::env::var_os("POEZIBROWSE_TIMING").is_some() {
                if let Some(t) = created.take() {
                    eprintln!("poezibrowse-timing-tab {} {}", t.elapsed().as_millis(), view.uri().unwrap_or_default());
                }
            }
            if event == LoadEvent::Finished {
                if let (Some(t), Some(uri)) = (started.take(), view.uri()) {
                    record_load_time(&uri, t.elapsed());
                }
            }
            if matches!(event, LoadEvent::Started | LoadEvent::Redirected) {
                if let (Some(engine), Some(uri)) = (engine.upgrade(), view.uri()) {
                    engine.update_blocking(&ucm, &uri);
                }
            }
            if event == LoadEvent::Committed {
                if let (Some(engine), Some(uri), Some((cert, flags))) = (engine.upgrade(), view.uri(), view.tls_info()) {
                    let host = config::host_of(&uri).unwrap_or_default();
                    let pem = cert.certificate_pem().map(|p| p.to_string()).unwrap_or_default();
                    if !flags.is_empty() && !engine.certificate_allowed(&host, &pem) {
                        view.stop_loading();
                        engine.certificate_error(view, &uri, &cert, flags);
                    }
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
        let engine = Rc::downgrade(&self.engine);
        v.connect_resource_load_started(move |_, _, request| {
            let Some(engine) = engine.upgrade() else { return };
            if !engine.log_requests.get() {
                return;
            }
            use webkit2gtk::URIRequestExt;
            let url = request.uri().map(|u| u.to_string()).unwrap_or_default();
            if url.is_empty() || url.starts_with("data:") || url.starts_with("blob:") {
                return;
            }
            let method = request.http_method().map(|m| m.to_string()).unwrap_or_else(|| "GET".into());
            let mut headers = Vec::new();
            if let Some(h) = request.http_headers() {
                h.foreach(|k, v| headers.push((k.to_string(), v.to_string())));
            }
            s.emit(PageEvent::Request(crate::intercept::RequestRecord { method, url, headers }));
        });
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
        let tor = self.engine.mode == Mode::Tor;
        let https_only = self.engine.https_only;
        let https_exceptions = self.engine.https_exceptions.clone();
        let block_local_net = self.engine.block_local_net;
        let engine = Rc::downgrade(&self.engine);
        v.connect_decide_policy(move |view, decision, kind| {
            let mut target = String::new();
            if matches!(kind, PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction) {
                target = decision
                    .downcast_ref::<NavigationPolicyDecision>()
                    .and_then(|n| n.navigation_action())
                    .and_then(|a| a.request())
                    .and_then(|r| r.uri())
                    .map(|u| u.to_string())
                    .unwrap_or_default();
                if let Some(nonce) = target.strip_prefix("poezi-certificate:") {
                    decision.ignore();
                    if let Some(uri) = engine.upgrade().and_then(|e| e.accept_certificate(nonce)) {
                        view.load_uri(&uri);
                    }
                    return true;
                }
                if !showing_blocked(&target) && engine.upgrade().is_some_and(|e| e.is_dangerous(&target)) {
                    decision.ignore();
                    show_blocked(view, &target, true);
                    s.emit(PageEvent::Blocked(target));
                    return true;
                }
                if block_local_net && !showing_blocked(&target) && is_local_navigation(view, &target) {
                    decision.ignore();
                    show_local_blocked(view, &target);
                    return true;
                }
                if kind == PolicyDecisionType::NavigationAction && (target.starts_with("http://") || target.starts_with("https://")) {
                    if let Some(engine) = engine.upgrade() {
                        if engine.intercept_bypass.borrow_mut().remove(&target) {
                            decision.use_();
                            return true;
                        }
                        if engine.intercepting.get() {
                            if let Some(callback) = engine.on_intercept.borrow().clone() {
                                let method = decision
                                    .downcast_ref::<NavigationPolicyDecision>()
                                    .and_then(|n| n.navigation_action())
                                    .and_then(|a| a.request())
                                    .and_then(|r| webkit2gtk::URIRequestExt::http_method(&r))
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| "GET".into());
                                callback(Held {
                                    decision: decision.clone(),
                                    view: view.clone(),
                                    method,
                                    url: target.clone(),
                                    bypass: engine.intercept_bypass.clone(),
                                });
                                return true;
                            }
                        }
                    }
                }
            }
            let eff_https = https_only && config::host_of(&target).map(|h| !host_in_list(&h, &https_exceptions)).unwrap_or(true);
            decide_policy(&s, view, decision, kind, tor, eff_https)
        });

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
            if hit.context_is_image() {
                arm_image_clipboard_fix();
            }
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

        let engine = Rc::downgrade(&self.engine);
        v.connect_permission_request(move |view, request| {
            let Some(engine) = engine.upgrade() else { return false };
            let store = engine.permission_store.borrow().clone();
            handle_permission(view, request, engine.mode, store)
        });

        let s = sink.clone();
        v.connect_load_failed(move |view, _, uri, error| {
            let message = error.message().to_ascii_lowercase();
            if !message.contains("content blocker") {
                return false;
            }
            show_blocked(view, uri, false);
            s.emit(PageEvent::Blocked(uri.to_string()));
            true
        });

        let engine = Rc::downgrade(&self.engine);
        v.connect_load_failed_with_tls_errors(move |view, uri, cert, flags| {
            if let Some(engine) = engine.upgrade() {
                engine.certificate_error(view, uri, cert, flags);
            }
            true
        });
    }
}

thread_local! {
    static LOAD_TIMES: RefCell<std::collections::VecDeque<(String, u128)>> = const { RefCell::new(std::collections::VecDeque::new()) };
}

fn record_load_time(uri: &str, elapsed: std::time::Duration) {
    if uri.starts_with("poezi:") || uri == "about:blank" {
        return;
    }
    let ms = elapsed.as_millis();
    if std::env::var_os("POEZIBROWSE_TIMING").is_some() {
        eprintln!("poezibrowse-timing {ms} {uri}");
    }
    LOAD_TIMES.with(|t| {
        let mut t = t.borrow_mut();
        t.push_front((uri.to_string(), ms));
        t.truncate(15);
    });
}

pub fn load_times() -> Vec<(String, u128)> {
    LOAD_TIMES.with(|t| t.borrow().iter().cloned().collect())
}

thread_local! {
    static SHOWING_BLOCKED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn showing_blocked(uri: &str) -> bool {
    SHOWING_BLOCKED.with(|s| {
        let mut s = s.borrow_mut();
        let found = s.iter().position(|u| u == uri);
        if let Some(i) = found {
            s.remove(i);
        }
        found.is_some()
    })
}

fn show_blocked(view: &WebView, uri: &str, dangerous: bool) {
    SHOWING_BLOCKED.with(|s| s.borrow_mut().push(uri.to_string()));
    let host = config::host_of(uri).unwrap_or_default();
    let html = crate::pages::error_page(
        "Seite blockiert",
        &format!(
            "PoeziBrowse hat <b>{}</b> blockiert. {}",
            crate::pages::escape(&host),
            if dangerous {
                "Die Adresse steht auf einer Liste bekannter Schad- und Phishing-Seiten (URLhaus, Phishing Filter). Solche Seiten verbreiten Viren oder stehlen Passwörter."
            } else {
                "Die Adresse steht auf einer Liste von Werbe- oder Tracking-Seiten."
            }
        ),
    );
    view.load_alternate_html(&html, uri, None);
}

fn certificate_problems(flags: gtk::gio::TlsCertificateFlags) -> String {
    use gtk::gio::TlsCertificateFlags as F;
    let reasons: Vec<&str> = [
        (F::UNKNOWN_CA, "es wurde nicht von einer bekannten Stelle ausgestellt (z. B. selbst signiert)"),
        (F::BAD_IDENTITY, "es gehört zu einer anderen Adresse"),
        (F::EXPIRED, "es ist abgelaufen"),
        (F::NOT_ACTIVATED, "es ist noch nicht gültig"),
        (F::REVOKED, "es wurde zurückgezogen"),
        (F::INSECURE, "es nutzt unsichere Verschlüsselung"),
    ]
    .into_iter()
    .filter(|(f, _)| flags.contains(*f))
    .map(|(_, t)| t)
    .collect();
    if reasons.is_empty() { "es konnte nicht geprüft werden".into() } else { reasons.join(", ") }
}

fn show_certificate_error(view: &WebView, uri: &str, flags: gtk::gio::TlsCertificateFlags, nonce: Option<&str>) {
    let host = config::host_of(uri).unwrap_or_default();
    let hint = match nonce {
        None => "Im Tor-Modus lassen sich ungültige Zertifikate nicht zulassen.".to_string(),
        Some(nonce) => format!(
            "Nur zulassen, wenn du dem Gerät vertraust – z. B. dein Router, NAS oder Server im Heimnetz. Sonst könnte jemand die Verbindung mitlesen. Wieder blockieren geht unter Einstellungen → Zertifikate.</p>\
             <p><a class=\"button danger\" href=\"poezi-certificate:{nonce}\">Trotzdem zulassen</a>"
        ),
    };
    let html = crate::pages::error_page(
        "Unsichere Verbindung",
        &format!(
            "Das Zertifikat von <b>{}</b> ist ungültig: {}. PoeziBrowse hat die Seite deshalb nicht geladen.</p><p>{hint}",
            crate::pages::escape(&host),
            certificate_problems(flags),
        ),
    );
    view.load_alternate_html(&html, uri, None);
}

fn needs_https(uri: &str) -> bool {
    uri.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("http://")) && !config::host_of(uri).is_some_and(|h| h.ends_with(".onion"))
}

fn host_in_list(host: &str, list: &[String]) -> bool {
    let host = host.trim_start_matches("www.");
    list.iter().any(|e| {
        let e = e.trim_start_matches("www.");
        host == e || host.ends_with(&format!(".{e}"))
    })
}

fn parse_ip_part(s: &str) -> Option<u64> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (!hex.is_empty()).then(|| u64::from_str_radix(hex, 16).ok()).flatten()
    } else if s.len() > 1 && s.starts_with('0') {
        u64::from_str_radix(&s[1..], 8).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

fn parse_ipv4(host: &str) -> Option<u32> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.is_empty() || parts.len() > 4 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let vals: Vec<u64> = parts.iter().map(|p| parse_ip_part(p)).collect::<Option<_>>()?;
    let ip: u64 = match vals.as_slice() {
        [a] => *a,
        [a, b] if *a <= 0xff && *b <= 0xff_ffff => (a << 24) | b,
        [a, b, c] if *a <= 0xff && *b <= 0xff && *c <= 0xffff => (a << 24) | (b << 16) | c,
        [a, b, c, d] if *a <= 0xff && *b <= 0xff && *c <= 0xff && *d <= 0xff => (a << 24) | (b << 16) | (c << 8) | d,
        _ => return None,
    };
    (ip <= 0xffff_ffff).then_some(ip as u32)
}

fn is_private_v4(ip: u32) -> bool {
    let first = ip >> 24;
    first == 127
        || first == 10
        || first == 0
        || (ip >> 16) == 0xC0A8
        || (ip >> 20) == 0xAC1
        || (ip >> 16) == 0xA9FE
}

pub fn is_local_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    if host == "::1" || host == "::" || host.starts_with("fc") || host.starts_with("fd") || host.starts_with("fe80") {
        return true;
    }
    if let Some(tail) = host.rsplit(':').next() {
        if tail.contains('.') && parse_ipv4(tail).is_some_and(is_private_v4) {
            return true;
        }
    }
    parse_ipv4(&host).is_some_and(is_private_v4)
}

fn is_local_navigation(view: &WebView, target: &str) -> bool {
    let Some(host) = config::host_of(target) else { return false };
    if !is_local_host(&host) {
        return false;
    }
    match view.uri().and_then(|u| config::host_of(&u)) {
        Some(current) => !is_local_host(&current),
        None => false,
    }
}

fn show_local_blocked(view: &WebView, uri: &str) {
    SHOWING_BLOCKED.with(|s| s.borrow_mut().push(uri.to_string()));
    let host = config::host_of(uri).unwrap_or_default();
    let html = crate::pages::error_page(
        "Lokale Adresse blockiert",
        &format!(
            "Diese Seite wollte eine Adresse in deinem lokalen Netzwerk öffnen: <b>{}</b>. PoeziBrowse hat das verhindert – so kann keine fremde Webseite heimlich deinen Router oder Geräte im Heimnetz ansteuern.</p>\
             <p>Wenn du die Adresse selbst erreichen willst, gib sie oben direkt in die Adressleiste ein. Den Schutz kannst du unter Einstellungen → Sicherheit abschalten.",
            crate::pages::escape(&host),
        ),
    );
    view.load_alternate_html(&html, uri, None);
}

fn upgrade_to_https(uri: &str) -> String {
    format!("https://{}", &uri[7..])
}

const INTERNAL_SCHEMES: &[&str] = &["http", "https", "file", "poezi", "about", "data", "blob", "javascript", "webkit-pdfjs-viewer"];

fn decide_policy(sink: &SinkSlot, view: &WebView, decision: &PolicyDecision, kind: PolicyDecisionType, tor: bool, https_only: bool) -> bool {
    match kind {
        PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction => {
            let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else { return false };
            let Some(mut action) = nav.navigation_action() else { return false };
            let target = action.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            let scheme = target.split_once(':').map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default();
            if !scheme.is_empty() && !INTERNAL_SCHEMES.contains(&scheme.as_str()) {
                decision.ignore();
                if !tor && action.is_user_gesture() {
                    let _ = gtk::gio::AppInfo::launch_default_for_uri(&target, None::<&gtk::gio::AppLaunchContext>);
                }
                return true;
            }
            if https_only && needs_https(&target) {
                decision.ignore();
                let clicked = action.navigation_type() == webkit2gtk::NavigationType::LinkClicked && action.frame_name().is_none();
                if kind == PolicyDecisionType::NavigationAction && clicked {
                    view.load_uri(&upgrade_to_https(&target));
                }
                return true;
            }
            if kind == PolicyDecisionType::NewWindowAction {
                return false;
            }
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

pub struct Held {
    decision: PolicyDecision,
    view: WebView,
    pub method: String,
    pub url: String,
    bypass: Rc<RefCell<std::collections::HashSet<String>>>,
}

impl Held {
    pub fn forward(self) {
        self.decision.use_();
    }

    pub fn drop_request(self) {
        self.decision.ignore();
    }

    pub fn forward_to(self, url: &str) {
        if url == self.url || url.is_empty() {
            self.decision.use_();
        } else {
            self.bypass.borrow_mut().insert(url.to_string());
            self.decision.ignore();
            self.view.load_uri(url);
        }
    }
}

#[derive(Clone)]
pub struct PermissionStore {
    pub get: Rc<dyn Fn(&str, PermissionKind) -> Option<bool>>,
    pub set: Rc<dyn Fn(&str, PermissionKind, bool)>,
}

fn handle_permission(view: &WebView, request: &PermissionRequest, mode: Mode, store: Option<PermissionStore>) -> bool {
    use webkit2gtk::{GeolocationPermissionRequest, NotificationPermissionRequest, PermissionRequestExt, PointerLockPermissionRequest, UserMediaPermissionRequest};
    if request.is::<PointerLockPermissionRequest>() {
        request.allow();
        return true;
    }
    let kind = if let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() {
        use webkit2gtk::UserMediaPermissionRequestExt;
        match (media.is_for_audio_device(), media.is_for_video_device()) {
            (true, true) => PermissionKind::CameraMicrophone,
            (false, true) => PermissionKind::Camera,
            _ => PermissionKind::Microphone,
        }
    } else if request.is::<GeolocationPermissionRequest>() {
        PermissionKind::Location
    } else if request.is::<NotificationPermissionRequest>() {
        PermissionKind::Notifications
    } else {
        request.deny();
        return true;
    };
    if mode == Mode::Tor {
        request.deny();
        return true;
    }
    let host = view.uri().and_then(|u| config::host_of(&u)).unwrap_or_default();
    let store = store.filter(|_| mode == Mode::Normal && !host.is_empty());
    if let Some(allow) = store.as_ref().and_then(|s| (s.get)(&host, kind)) {
        if allow {
            request.allow();
        } else {
            request.deny();
        }
        return true;
    }
    let parent = view.toplevel().and_then(|w| w.downcast::<gtk::Window>().ok());
    let dialog = gtk::MessageDialog::new(
        parent.as_ref(),
        gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Question,
        gtk::ButtonsType::None,
        &format!("{host} möchte {} nutzen.", kind.label()),
    );
    let remember = gtk::CheckButton::with_label("Entscheidung für diese Seite merken");
    remember.set_active(true);
    if store.is_some() {
        dialog.message_area().downcast::<gtk::Box>().ok().inspect(|area| area.pack_start(&remember, false, false, 6));
    }
    dialog.add_buttons(&[("Ablehnen", gtk::ResponseType::Reject), ("Erlauben", gtk::ResponseType::Accept)]);
    let request = request.clone();
    dialog.connect_response(move |d, response| {
        let allow = response == gtk::ResponseType::Accept;
        if allow {
            request.allow();
        } else {
            request.deny();
        }
        if remember.is_active() && matches!(response, gtk::ResponseType::Accept | gtk::ResponseType::Reject) {
            if let Some(store) = &store {
                (store.set)(&host, kind, allow);
            }
        }
        d.close();
    });
    dialog.show_all();
    true
}

thread_local! {
    static IMAGE_COPY_ARMED: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    static IMAGE_COPY_WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn arm_image_clipboard_fix() {
    IMAGE_COPY_ARMED.with(|a| a.set(Some(std::time::Instant::now())));
    if IMAGE_COPY_WATCHING.with(|w| w.replace(true)) {
        return;
    }
    let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clipboard.connect_local("owner-change", false, |_| {
        let armed = IMAGE_COPY_ARMED.with(|a| a.take()).is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(60));
        if !armed {
            return None;
        }
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        clipboard.request_contents(&gtk::gdk::Atom::intern("TARGETS"), |clipboard, data| {
            let targets = data.targets().unwrap_or_default();
            let has = |name: &str| targets.iter().any(|t| t.name() == name);
            if has("image/png") && has("text/uri-list") {
                clipboard.request_image(|clipboard, image| {
                    if let Some(image) = image {
                        clipboard.set_image(image);
                    }
                });
            }
        });
        None
    });
}

fn apply_proxy(data_manager: &WebsiteDataManager, config: &Config) {
    match config.proxy_mode {
        ProxyMode::System => data_manager.set_network_proxy_settings(NetworkProxyMode::Default, None),
        ProxyMode::None => data_manager.set_network_proxy_settings(NetworkProxyMode::NoProxy, None),
        ProxyMode::Custom => {
            if config::valid_proxy_url(&config.proxy_url) {
                let ignore: Vec<&str> = config.proxy_ignore.iter().map(String::as_str).collect();
                let mut settings = NetworkProxySettings::new(Some(&config.proxy_url), &ignore);
                data_manager.set_network_proxy_settings(NetworkProxyMode::Custom, Some(&mut settings));
            } else {
                data_manager.set_network_proxy_settings(NetworkProxyMode::NoProxy, None);
            }
        }
    }
}

fn set_tor_proxy(data_manager: &WebsiteDataManager, port: u16) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut bytes = [0u8; 12];
    let _ = getrandom::fill(&mut bytes);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let user: String = bytes.iter().map(|b| format!("{b:02x}")).chain(std::iter::once(format!("-{n}"))).collect();
    let proxy = format!("socks5://{user}:poezi@127.0.0.1:{port}");
    let mut proxy_settings = NetworkProxySettings::new(Some(&proxy), &[]);
    data_manager.set_network_proxy_settings(NetworkProxyMode::Custom, Some(&mut proxy_settings));
}

fn set_feature(settings: &Settings, identifier: &str, enabled: bool) -> bool {
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
                set(settings.to_glib_none().0, feature, enabled as i32);
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
        if self.0.engine.mode == Mode::Tor && self.0.engine.tor_https_only && needs_https(uri) {
            self.0.view.load_uri(&upgrade_to_https(uri));
        } else {
            self.0.view.load_uri(uri);
        }
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
    use super::{is_local_host, needs_https, site_key, upgrade_to_https};

    #[test]
    fn local_hosts() {
        for h in ["localhost", "127.0.0.1", "10.1.2.3", "192.168.0.5", "172.16.0.1", "172.31.255.255", "169.254.1.1",
                  "nas.local", "::1", "fd00::1", "2130706433", "0x7f.0.0.1", "0177.0.0.1", "127.1", "127.0.1",
                  "::ffff:127.0.0.1", "[::1]"] {
            assert!(is_local_host(h), "{h} should be local");
        }
        for h in ["example.com", "8.8.8.8", "172.32.0.1", "11.0.0.1", "github.com", "192.169.0.1", "1.2.3.4"] {
            assert!(!is_local_host(h), "{h} should not be local");
        }
    }

    #[test]
    fn https_upgrade() {
        assert!(needs_https("http://example.com/a?b"));
        assert!(!needs_https("https://example.com/"));
        assert!(!needs_https("http://abcdefghijklmnop.onion/"));
        assert_eq!(upgrade_to_https("http://example.com/a"), "https://example.com/a");
    }

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
