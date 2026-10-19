use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use gtk::glib;
use gtk::glib::translate::{FromGlibPtrFull, ToGlibPtr};
use webkit2gtk::{CookieManager, CookieManagerExt};
use zeroize::Zeroizing;

use crate::config;
use crate::passwords::secret::{self, Kind, Store};

fn file() -> PathBuf {
    config::data_dir().join("cookies.enc")
}

fn key(create: bool) -> Option<Zeroizing<[u8; 32]>> {
    if let Some(hex) = secret::load(Kind::CookieKey) {
        let mut key = Zeroizing::new([0u8; 32]);
        if hex.len() != 64 {
            return None;
        }
        for (i, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        return Some(key);
    }
    if !create {
        return None;
    }
    let mut key = Zeroizing::new([0u8; 32]);
    getrandom::fill(key.as_mut()).ok()?;
    let hex = Zeroizing::new(key.iter().map(|b| format!("{b:02x}")).collect::<String>());
    match secret::store(Kind::CookieKey, &hex) {
        Store::Memory => {
            secret::forget(Kind::CookieKey);
            None
        }
        _ => Some(key),
    }
}

fn encrypt(key: &[u8; 32], plain: &[u8]) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).ok()?;
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).ok()?;
    let mut out = nonce.to_vec();
    out.extend(cipher.encrypt(Nonce::from_slice(&nonce), plain).ok()?);
    Some(out)
}

fn decrypt(key: &[u8; 32], data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    if data.len() < 12 + 16 {
        return None;
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key).ok()?;
    cipher.decrypt(Nonce::from_slice(&data[..12]), &data[12..]).ok().map(Zeroizing::new)
}

pub fn forget() {
    let _ = std::fs::remove_file(file());
}

pub fn restore(manager: &CookieManager) {
    let Ok(data) = std::fs::read(file()) else { return };
    let Some(key) = key(false) else { return };
    let Some(plain) = decrypt(&key, &data) else {
        eprintln!("poezibrowse: saved logins could not be decrypted");
        return;
    };
    let Ok(entries) = serde_json::from_slice::<Vec<(String, String)>>(&plain) else { return };
    for (domain, header) in entries {
        let host = domain.trim_start_matches('.');
        let Ok(origin) = glib::Uri::parse(&format!("https://{host}/"), glib::UriFlags::NONE) else { continue };
        if let Some(mut cookie) = soup::Cookie::parse(&header, Some(&origin)) {
            manager.add_cookie(&mut cookie, None::<&gtk::gio::Cancellable>, |_| {});
        }
    }
}

fn write(cookies: Vec<soup::Cookie>) {
    let now = glib::DateTime::now_utc().ok();
    let entries: Vec<(String, String)> = cookies
        .into_iter()
        .filter_map(|mut c| {
            let expires = c.expires()?;
            if now.as_ref().is_some_and(|n| expires <= *n) {
                return None;
            }
            Some((c.domain()?.to_string(), c.to_set_cookie_header()?.to_string()))
        })
        .collect();
    if entries.is_empty() {
        forget();
        return;
    }
    let Some(key) = key(true) else { return };
    let Ok(plain) = serde_json::to_vec(&entries).map(Zeroizing::new) else { return };
    let Some(data) = encrypt(&key, &plain) else { return };
    let path = file();
    let tmp = path.with_extension("enc.tmp");
    if config::write_private(&tmp, &data).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

type Callback = Box<dyn FnOnce(Vec<soup::Cookie>)>;

fn all_cookies(manager: &CookieManager, done: impl FnOnce(Vec<soup::Cookie>) + 'static) -> bool {
    type GetAll = unsafe extern "C" fn(
        *mut webkit2gtk::ffi::WebKitCookieManager,
        *mut gtk::gio::ffi::GCancellable,
        gtk::gio::ffi::GAsyncReadyCallback,
        glib::ffi::gpointer,
    );
    unsafe extern "C" fn finished(source: *mut glib::gobject_ffi::GObject, result: *mut gtk::gio::ffi::GAsyncResult, data: glib::ffi::gpointer) {
        type Finish = unsafe extern "C" fn(
            *mut webkit2gtk::ffi::WebKitCookieManager,
            *mut gtk::gio::ffi::GAsyncResult,
            *mut *mut glib::ffi::GError,
        ) -> *mut glib::ffi::GList;
        let callback = unsafe { Box::from_raw(data as *mut Callback) };
        let mut cookies = Vec::new();
        if let Some(finish) = crate::system::symbol("webkit_cookie_manager_get_all_cookies_finish") {
            let finish: Finish = unsafe { std::mem::transmute(finish) };
            let mut error = std::ptr::null_mut();
            let mut list = unsafe { finish(source as *mut _, result, &mut error) };
            if !error.is_null() {
                unsafe { glib::ffi::g_error_free(error) };
            }
            let head = list;
            while !list.is_null() {
                let cookie = unsafe { (*list).data } as *mut soup::ffi::SoupCookie;
                if !cookie.is_null() {
                    cookies.push(unsafe { soup::Cookie::from_glib_full(cookie) });
                }
                list = unsafe { (*list).next };
            }
            if !head.is_null() {
                unsafe { glib::ffi::g_list_free(head) };
            }
        }
        callback(cookies);
    }
    let Some(get_all) = crate::system::symbol("webkit_cookie_manager_get_all_cookies") else { return false };
    let get_all: GetAll = unsafe { std::mem::transmute(get_all) };
    let data: Box<Callback> = Box::new(Box::new(done));
    let ptr: *mut webkit2gtk::ffi::WebKitCookieManager = manager.to_glib_none().0;
    unsafe { get_all(ptr, std::ptr::null_mut(), Some(finished), Box::into_raw(data) as glib::ffi::gpointer) };
    true
}

pub fn supported() -> bool {
    crate::system::symbol("webkit_cookie_manager_get_all_cookies").is_some()
}

pub struct Keeper {
    manager: CookieManager,
    pending: Rc<Cell<bool>>,
}

impl Keeper {
    pub fn new(manager: CookieManager) -> Rc<Keeper> {
        restore(&manager);
        let keeper = Rc::new(Keeper { manager: manager.clone(), pending: Rc::new(Cell::new(false)) });
        let weak = Rc::downgrade(&keeper);
        manager.connect_changed(move |_| {
            if let Some(k) = weak.upgrade() {
                k.save_soon();
            }
        });
        keeper
    }

    fn save_soon(self: &Rc<Self>) {
        if self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_secs(5), move || {
            if let Some(k) = weak.upgrade() {
                k.pending.set(false);
                all_cookies(&k.manager, write);
            }
        });
    }

    pub fn save_now(&self) {
        let done = Rc::new(Cell::new(false));
        let flag = done.clone();
        let result: Rc<RefCell<Option<Vec<soup::Cookie>>>> = Rc::default();
        let slot = result.clone();
        if !all_cookies(&self.manager, move |cookies| {
            *slot.borrow_mut() = Some(cookies);
            flag.set(true);
        }) {
            return;
        }
        let context = glib::MainContext::default();
        let start = std::time::Instant::now();
        while !done.get() && start.elapsed() < Duration::from_secs(3) {
            context.iteration(true);
        }
        if let Some(cookies) = result.take() {
            write(cookies);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decrypt, encrypt};

    #[test]
    fn roundtrip() {
        let key = [7u8; 32];
        let data = encrypt(&key, b"user_session=geheim").unwrap();
        assert!(!data.windows(6).any(|w| w == b"geheim"));
        assert_eq!(decrypt(&key, &data).unwrap().as_slice(), b"user_session=geheim");
        assert!(decrypt(&[8u8; 32], &data).is_none());
        let mut broken = data.clone();
        broken[20] ^= 1;
        assert!(decrypt(&key, &broken).is_none());
    }
}
