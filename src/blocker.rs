use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use gtk::gio;
use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use webkit2gtk::ffi;
use webkit2gtk::UserContentManager;

use crate::config;

pub const NETWORK: &str = "poezi-network";
pub const COSMETIC: &str = "poezi-cosmetic";
pub const COSMETIC_FAST: &str = "poezi-cosmetic-fast";
pub const MALWARE: &str = "poezi-malware";
const IDENTIFIERS: [&str; 4] = [NETWORK, COSMETIC, COSMETIC_FAST, MALWARE];
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const LISTS: &[(&str, &str)] = &[
    ("EasyList", "https://easylist.to/easylist/easylist.txt"),
    ("EasyPrivacy", "https://easylist.to/easylist/easyprivacy.txt"),
    ("EasyList Germany", "https://easylist.to/easylistgermany/easylistgermany.txt"),
];
pub const MALWARE_LISTS: &[(&str, &str)] = &[
    ("URLhaus", "https://malware-filter.gitlab.io/malware-filter/urlhaus-filter-agh-online.txt"),
    ("Phishing Filter", "https://malware-filter.gitlab.io/malware-filter/phishing-filter-agh.txt"),
];

pub struct Filter(ptr::NonNull<ffi::WebKitUserContentFilter>);

impl Filter {
    unsafe fn from_raw_full(p: *mut ffi::WebKitUserContentFilter) -> Option<Filter> {
        ptr::NonNull::new(p).map(Filter)
    }
}

impl Clone for Filter {
    fn clone(&self) -> Self {
        unsafe { Filter(ptr::NonNull::new_unchecked(ffi::webkit_user_content_filter_ref(self.0.as_ptr()))) }
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        unsafe { ffi::webkit_user_content_filter_unref(self.0.as_ptr()) }
    }
}

pub fn add_filter(ucm: &UserContentManager, filter: &Filter) {
    let ucm_ptr: *mut ffi::WebKitUserContentManager = ucm.to_glib_none().0;
    unsafe { ffi::webkit_user_content_manager_add_filter(ucm_ptr, filter.0.as_ptr()) }
}

pub fn remove_filters(ucm: &UserContentManager) {
    use webkit2gtk::UserContentManagerExt;
    ucm.remove_all_filters();
}

pub struct Blocker {
    store: *mut ffi::WebKitUserContentFilterStore,
    pub filters: RefCell<Vec<(String, Filter)>>,
    dangerous: RefCell<std::collections::HashSet<String>>,
    on_ready: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Drop for Blocker {
    fn drop(&mut self) {
        unsafe { glib::gobject_ffi::g_object_unref(self.store as *mut _) }
    }
}

fn dir() -> PathBuf {
    config::cache_dir().join("filters")
}

impl Blocker {
    pub fn new() -> Rc<Blocker> {
        let path = dir();
        let _ = config::private_dir(&path);
        let c_path = CString::new(path.to_string_lossy().as_bytes()).unwrap_or_default();
        let store = unsafe { ffi::webkit_user_content_filter_store_new(c_path.as_ptr()) };
        Rc::new(Blocker { store, filters: RefCell::new(Vec::new()), dangerous: RefCell::new(read_hosts()), on_ready: RefCell::new(Vec::new()) })
    }

    pub fn connect_ready(&self, f: impl Fn() + 'static) {
        if !self.filters.borrow().is_empty() {
            f();
        }
        self.on_ready.borrow_mut().push(Box::new(f));
    }

    pub fn is_dangerous(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let hosts = self.dangerous.borrow();
        let mut rest = host.as_str();
        loop {
            if hosts.contains(rest) {
                return true;
            }
            match rest.split_once('.') {
                Some((_, parent)) if parent.contains('.') || !parent.is_empty() => rest = parent,
                _ => return false,
            }
        }
    }

    fn set_filters(&self, filters: Vec<(String, Filter)>) {
        *self.filters.borrow_mut() = filters;
        *self.dangerous.borrow_mut() = read_hosts();
        for f in self.on_ready.borrow().iter() {
            f();
        }
    }

    pub fn select(&self, ids: &[&str]) -> Vec<Filter> {
        self.filters.borrow().iter().filter(|(id, _)| ids.contains(&id.as_str())).map(|(_, f)| f.clone()).collect()
    }

    pub fn start(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        let loaded: Rc<RefCell<Vec<(String, Filter)>>> = Rc::default();
        let pending = Rc::new(std::cell::Cell::new(IDENTIFIERS.len()));
        for id in IDENTIFIERS {
            let (me, loaded, pending) = (me.clone(), loaded.clone(), pending.clone());
            self.load(id, move |filter| {
                loaded.borrow_mut().extend(filter.map(|f| (id.to_string(), f)));
                pending.set(pending.get() - 1);
                if pending.get() > 0 {
                    return;
                }
                let Some(me) = me.upgrade() else { return };
                let filters = loaded.take();
                let complete = filters.len() == IDENTIFIERS.len();
                if !filters.is_empty() {
                    me.set_filters(filters);
                }
                if !complete || is_stale() {
                    me.update();
                }
            });
        }
    }

    pub fn update(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let result = gio::spawn_blocking(build_in_child_process).await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    eprintln!("poezibrowse: filter lists not updated: {e}");
                    return;
                }
                Err(_) => return,
            }
            let Some(me) = me.upgrade() else { return };
            let saved: Rc<RefCell<Vec<(String, Filter)>>> = Rc::default();
            let pending = Rc::new(std::cell::Cell::new(IDENTIFIERS.len()));
            for id in IDENTIFIERS {
                let (me2, saved, pending) = (Rc::downgrade(&me), saved.clone(), pending.clone());
                let file = json_path(id);
                let done_file = file.clone();
                me.save_from_file(id, &file, move |filter| {
                    let _ = std::fs::remove_file(&done_file);
                    saved.borrow_mut().extend(filter.map(|f| (id.to_string(), f)));
                    pending.set(pending.get() - 1);
                    if pending.get() > 0 {
                        return;
                    }
                    let filters = saved.take();
                    if let Some(me) = me2.upgrade() {
                        if filters.len() == IDENTIFIERS.len() {
                            let _ = std::fs::write(dir().join("updated"), b"");
                            me.set_filters(filters);
                        }
                    }
                });
            }
        });
    }

    fn load(&self, identifier: &str, done: impl FnOnce(Option<Filter>) + 'static) {
        type Cb = Box<dyn FnOnce(Option<Filter>)>;
        unsafe extern "C" fn finished(
            source: *mut glib::gobject_ffi::GObject,
            result: *mut gio::ffi::GAsyncResult,
            data: glib::ffi::gpointer,
        ) {
            unsafe {
                let done: Box<Cb> = Box::from_raw(data as *mut Cb);
                let mut error = ptr::null_mut();
                let filter = ffi::webkit_user_content_filter_store_load_finish(source as *mut _, result, &mut error);
                if !error.is_null() {
                    glib::ffi::g_error_free(error);
                }
                done(Filter::from_raw_full(filter));
            }
        }
        let id = CString::new(identifier).unwrap();
        let data: Box<Cb> = Box::new(Box::new(done));
        unsafe {
            ffi::webkit_user_content_filter_store_load(
                self.store,
                id.as_ptr(),
                ptr::null_mut(),
                Some(finished),
                Box::into_raw(data) as glib::ffi::gpointer,
            );
        }
    }

    fn save_from_file(&self, identifier: &str, file: &std::path::Path, done: impl FnOnce(Option<Filter>) + 'static) {
        type Cb = Box<dyn FnOnce(Option<Filter>)>;
        unsafe extern "C" fn finished(
            source: *mut glib::gobject_ffi::GObject,
            result: *mut gio::ffi::GAsyncResult,
            data: glib::ffi::gpointer,
        ) {
            unsafe {
                let done: Box<Cb> = Box::from_raw(data as *mut Cb);
                let mut error = ptr::null_mut();
                let filter = ffi::webkit_user_content_filter_store_save_from_file_finish(source as *mut _, result, &mut error);
                if !error.is_null() {
                    let message = CStr::from_ptr((*error).message).to_string_lossy();
                    eprintln!("poezibrowse: WebKit could not compile the filter list: {message}");
                    glib::ffi::g_error_free(error);
                }
                done(Filter::from_raw_full(filter));
            }
        }
        let id = CString::new(identifier).unwrap();
        let file = gio::File::for_path(file);
        let data: Box<Cb> = Box::new(Box::new(done));
        unsafe {
            ffi::webkit_user_content_filter_store_save_from_file(
                self.store,
                id.as_ptr(),
                file.to_glib_none().0,
                ptr::null_mut(),
                Some(finished),
                Box::into_raw(data) as glib::ffi::gpointer,
            );
        }
    }
}

fn is_stale() -> bool {
    std::fs::metadata(dir().join("updated"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_none_or(|age| age > MAX_AGE)
}

fn hosts_path() -> std::path::PathBuf {
    dir().join("dangerous-hosts.txt")
}

fn read_hosts() -> std::collections::HashSet<String> {
    std::fs::read_to_string(hosts_path()).map(|t| t.lines().map(String::from).collect()).unwrap_or_default()
}

fn json_path(identifier: &str) -> std::path::PathBuf {
    dir().join(format!("{identifier}.json"))
}

fn build_in_child_process() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let status = std::process::Command::new(exe)
        .arg(BUILD_FILTERS_ARG)
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("filter builder exited with {status}")) }
}

pub const BUILD_FILTERS_ARG: &str = "--build-filter-lists";

pub fn build_filter_files() -> bool {
    let result = build_rules().and_then(|(lists, hosts)| {
        for (id, json) in IDENTIFIERS.into_iter().zip(lists) {
            config::write_private(&json_path(id), json.as_bytes()).map_err(|e| e.to_string())?;
        }
        if !hosts.is_empty() {
            config::write_private(&hosts_path(), hosts.join("\n").as_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(())
    });
    if let Err(e) = &result {
        eprintln!("poezibrowse: {e}");
    }
    result.is_ok()
}

fn rules_from(lists: &[(&str, &str)], hosts: &mut Vec<String>) -> (Vec<adblock::content_blocking::CbRule>, usize) {
    let mut set = adblock::lists::FilterSet::new(true);
    let mut loaded = 0;
    for (name, url) in lists {
        match download(url) {
            Ok(text) => {
                hosts.extend(text.lines().filter_map(|l| l.strip_prefix("||")?.strip_suffix('^')).filter(|h| !h.contains(['/', '*', '$'])).map(str::to_ascii_lowercase));
                set.add_filter_list(text, adblock::lists::ParseOptions::default());
                loaded += 1;
            }
            Err(e) => eprintln!("poezibrowse: could not download {name}: {e}"),
        }
    }
    if loaded == 0 {
        return (Vec::new(), 0);
    }
    (set.into_content_blocking().map(|(rules, _)| rules).unwrap_or_default(), loaded)
}

fn build_rules() -> Result<([String; 4], Vec<String>), String> {
    let (rules, ads_loaded) = rules_from(LISTS, &mut Vec::new());
    let mut hosts = Vec::new();
    let (mut malware, malware_loaded) = rules_from(MALWARE_LISTS, &mut hosts);
    for rule in malware.iter_mut() {
        rule.trigger.resource_type = None;
        rule.trigger.load_type = Vec::new();
    }
    if ads_loaded == 0 && malware_loaded == 0 {
        return Err("no filter list could be downloaded".into());
    }
    let (cosmetic, network): (Vec<_>, Vec<_>) = rules
        .into_iter()
        .partition(|r| matches!(r.action.typ, adblock::content_blocking::CbType::CssDisplayNone));
    let fast: Vec<_> = cosmetic.iter().filter(|r| r.trigger.if_domain.as_ref().is_some_and(|d| !d.is_empty())).cloned().collect();
    let json = |rules: &Vec<adblock::content_blocking::CbRule>| serde_json::to_string(rules).map_err(|e| e.to_string());
    Ok(([json(&network)?, json(&cosmetic)?, json(&fast)?, json(&malware)?], hosts))
}

pub fn download(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(format!("{}/{}", config::APP_NAME, config::VERSION))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())
}
