use std::path::{Path, PathBuf};

use gtk::prelude::*;

use crate::config;

const ICONS: &[(&str, &[u8])] = &[
    ("audio-volume-high-symbolic", include_bytes!("../assets/icons/audio-volume-high-symbolic.svg")),
    ("audio-volume-muted-symbolic", include_bytes!("../assets/icons/audio-volume-muted-symbolic.svg")),
    ("dialog-password-symbolic", include_bytes!("../assets/icons/dialog-password-symbolic.svg")),
    ("document-open-recent-symbolic", include_bytes!("../assets/icons/document-open-recent-symbolic.svg")),
    ("emblem-system-symbolic", include_bytes!("../assets/icons/emblem-system-symbolic.svg")),
    ("folder-download-symbolic", include_bytes!("../assets/icons/folder-download-symbolic.svg")),
    ("go-down-symbolic", include_bytes!("../assets/icons/go-down-symbolic.svg")),
    ("go-next-symbolic", include_bytes!("../assets/icons/go-next-symbolic.svg")),
    ("go-previous-symbolic", include_bytes!("../assets/icons/go-previous-symbolic.svg")),
    ("go-up-symbolic", include_bytes!("../assets/icons/go-up-symbolic.svg")),
    ("help-about-symbolic", include_bytes!("../assets/icons/help-about-symbolic.svg")),
    ("list-add-symbolic", include_bytes!("../assets/icons/list-add-symbolic.svg")),
    ("non-starred-symbolic", include_bytes!("../assets/icons/non-starred-symbolic.svg")),
    ("open-menu-symbolic", include_bytes!("../assets/icons/open-menu-symbolic.svg")),
    ("poezi-page-symbolic", include_bytes!("../assets/icons/poezi-page-symbolic.svg")),
    ("process-stop-symbolic", include_bytes!("../assets/icons/process-stop-symbolic.svg")),
    ("security-high-symbolic", include_bytes!("../assets/icons/security-high-symbolic.svg")),
    ("security-low-symbolic", include_bytes!("../assets/icons/security-low-symbolic.svg")),
    ("starred-symbolic", include_bytes!("../assets/icons/starred-symbolic.svg")),
    ("user-bookmarks-symbolic", include_bytes!("../assets/icons/user-bookmarks-symbolic.svg")),
    ("view-refresh-symbolic", include_bytes!("../assets/icons/view-refresh-symbolic.svg")),
    ("weather-clear-night-symbolic", include_bytes!("../assets/icons/weather-clear-night-symbolic.svg")),
    ("window-close-symbolic", include_bytes!("../assets/icons/window-close-symbolic.svg")),
];

pub fn install_fallback_icons() {
    let dir = config::cache_dir().join(format!("icons-{}", config::VERSION));
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for (name, svg) in ICONS {
        let path = dir.join(format!("{name}.svg"));
        if !path.is_file() {
            let _ = std::fs::write(&path, svg);
        }
    }
    if let Some(theme) = gtk::IconTheme::default() {
        theme.append_search_path(&dir);
    }
}

struct Plugin {
    file: &'static str,
    purpose: &'static str,
    critical: bool,
}

const PLUGINS: &[Plugin] = &[
    Plugin { file: "libgstautodetect.so", purpose: "Tonausgabe – ohne sie stürzen Seiten mit Video oder Ton ab", critical: true },
    Plugin { file: "libgstisomp4.so", purpose: "MP4-Videos", critical: false },
    Plugin { file: "libgstmatroska.so", purpose: "WebM-Videos", critical: false },
    Plugin { file: "libgstvpx.so", purpose: "VP8/VP9-Videos (YouTube)", critical: false },
    Plugin { file: "libgstlibav.so", purpose: "H.264/AAC-Videos", critical: false },
];

fn plugin_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for var in ["GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SYSTEM_PATH", "GST_PLUGIN_PATH_1_0", "GST_PLUGIN_PATH"] {
        if let Some(value) = std::env::var_os(var) {
            dirs.extend(std::env::split_paths(&value));
        }
    }
    for dir in [
        "/usr/lib/gstreamer-1.0",
        "/usr/lib64/gstreamer-1.0",
        "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
        "/usr/local/lib/gstreamer-1.0",
        "/app/lib/gstreamer-1.0",
    ] {
        dirs.push(PathBuf::from(dir));
    }
    dirs
}

fn has_plugin(dirs: &[PathBuf], file: &str) -> bool {
    dirs.iter().any(|d| d.join(file).is_file())
}

pub struct MediaReport {
    pub missing: Vec<(&'static str, &'static str)>,
    pub critical: bool,
}

pub fn media_report() -> MediaReport {
    let dirs = plugin_dirs();
    if !dirs.iter().any(|d| d.is_dir()) {
        return MediaReport { missing: Vec::new(), critical: false };
    }
    let missing: Vec<&Plugin> = PLUGINS.iter().filter(|p| !has_plugin(&dirs, p.file)).collect();
    MediaReport {
        critical: missing.iter().any(|p| p.critical),
        missing: missing.iter().map(|p| (p.file, p.purpose)).collect(),
    }
}

pub fn tls_available() -> bool {
    gtk::gio::TlsBackend::default().supports_tls()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Arch,
    Debian,
    Fedora,
    Suse,
    Other,
}

pub fn os_release() -> (String, String, String) {
    let text = std::fs::read_to_string("/etc/os-release").or_else(|_| std::fs::read_to_string("/usr/lib/os-release")).unwrap_or_default();
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
    (get("ID"), get("ID_LIKE"), get("PRETTY_NAME"))
}

pub fn family() -> Family {
    let (id, like, _) = os_release();
    let all = format!("{id} {like}").to_lowercase();
    if all.contains("arch") || ["cachyos", "garuda", "manjaro", "endeavouros"].contains(&id.as_str()) {
        Family::Arch
    } else if all.contains("debian") || all.contains("ubuntu") {
        Family::Debian
    } else if all.contains("fedora") || all.contains("rhel") {
        Family::Fedora
    } else if all.contains("suse") {
        Family::Suse
    } else {
        Family::Other
    }
}

pub fn media_install_command() -> &'static str {
    match family() {
        Family::Arch => "sudo pacman -S --needed gst-plugins-good gst-plugins-base gst-libav",
        Family::Debian => "sudo apt install gstreamer1.0-plugins-good gstreamer1.0-plugins-base gstreamer1.0-libav",
        Family::Fedora => "sudo dnf install gstreamer1-plugins-good gstreamer1-plugins-base gstreamer1-plugin-libav",
        Family::Suse => "sudo zypper install gstreamer-plugins-good gstreamer-plugins-base gstreamer-plugins-libav",
        Family::Other => "GStreamer-Pakete „plugins-good“, „plugins-base“ und „libav“ installieren",
    }
}

pub fn tor_install_command() -> &'static str {
    match family() {
        Family::Arch => "sudo pacman -S tor",
        Family::Debian => "sudo apt install tor",
        Family::Fedora => "sudo dnf install tor",
        Family::Suse => "sudo zypper install tor",
        Family::Other => "Paket „tor“ installieren",
    }
}

pub fn tls_install_command() -> &'static str {
    match family() {
        Family::Arch => "sudo pacman -S glib-networking",
        Family::Debian => "sudo apt install glib-networking",
        Family::Fedora => "sudo dnf install glib-networking",
        Family::Suse => "sudo zypper install glib-networking",
        Family::Other => "Paket „glib-networking“ installieren",
    }
}

pub fn webkit_at_least(major: u32, minor: u32) -> bool {
    let (ma, mi) = unsafe { (webkit2gtk::ffi::webkit_get_major_version(), webkit2gtk::ffi::webkit_get_minor_version()) };
    (ma, mi) >= (major, minor)
}

pub fn nvidia() -> bool {
    Path::new("/proc/driver/nvidia").exists()
}

pub fn wayland_session() -> bool {
    let x11_forced = std::env::var("GDK_BACKEND").is_ok_and(|b| b.starts_with("x11"));
    !x11_forced && (std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland"))
}

pub fn dmabuf_disabled() -> bool {
    std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_ok_and(|v| v != "0")
}

pub fn fonts_install_command() -> &'static str {
    match family() {
        Family::Arch => "sudo pacman -S fontconfig ttf-dejavu noto-fonts",
        Family::Debian => "sudo apt install --reinstall fontconfig-config fonts-dejavu-core fonts-noto-core",
        Family::Fedora => "sudo dnf install fontconfig dejavu-sans-fonts dejavu-serif-fonts && sudo dnf reinstall fontconfig",
        Family::Suse => "sudo zypper install -f fontconfig dejavu-fonts",
        Family::Other => "Paket „fontconfig“ neu installieren und eine Schrift wie DejaVu oder Noto installieren",
    }
}

pub fn symbol(name: &str) -> Option<*mut std::ffi::c_void> {
    unsafe extern "C" {
        fn dlsym(handle: *mut std::ffi::c_void, symbol: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    }
    let name = std::ffi::CString::new(name).ok()?;
    let p = unsafe { dlsym(std::ptr::null_mut(), name.as_ptr()) };
    (!p.is_null()).then_some(p)
}

pub fn fonts_usable() -> bool {
    type Parse = unsafe extern "C" fn(*const u8) -> *mut std::ffi::c_void;
    type Substitute = unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i32) -> i32;
    type Default = unsafe extern "C" fn(*mut std::ffi::c_void);
    type GetString = unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, i32, *mut *const u8) -> i32;
    type Destroy = unsafe extern "C" fn(*mut std::ffi::c_void);
    let (Some(parse), Some(substitute), Some(default), Some(get), Some(destroy)) = (
        symbol("FcNameParse"),
        symbol("FcConfigSubstitute"),
        symbol("FcDefaultSubstitute"),
        symbol("FcPatternGetString"),
        symbol("FcPatternDestroy"),
    ) else {
        return true;
    };
    unsafe {
        let parse: Parse = std::mem::transmute(parse);
        let substitute: Substitute = std::mem::transmute(substitute);
        let default: Default = std::mem::transmute(default);
        let get: GetString = std::mem::transmute(get);
        let destroy: Destroy = std::mem::transmute(destroy);
        ["serif", "sans-serif"].iter().all(|generic| {
            let name = format!("{generic}\0");
            let pattern = parse(name.as_ptr());
            if pattern.is_null() {
                return false;
            }
            substitute(std::ptr::null_mut(), pattern, 0);
            default(pattern);
            let mut families = 0;
            let mut value = std::ptr::null();
            while get(pattern, b"family\0".as_ptr(), families, &mut value) == 0 {
                families += 1;
            }
            destroy(pattern);
            families > 1
        })
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(program).is_file()))
}

pub fn crash_log_path() -> PathBuf {
    config::cache_dir().join("crashes.log")
}

pub fn log_crash(uri: &str, reason: &str) {
    use std::io::Write;
    let host = config::host_of(uri).unwrap_or_default();
    let when = gtk::glib::DateTime::now_local().ok().and_then(|d| d.format("%Y-%m-%d %H:%M:%S").ok()).map(|s| s.to_string()).unwrap_or_default();
    let line = format!("{when}  {reason}  {host}  WebKitGTK {}\n", webkit_version());
    let path = crash_log_path();
    if let Some(dir) = path.parent() {
        let _ = config::private_dir(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn webkit_version() -> String {
    unsafe {
        format!(
            "{}.{}.{}",
            webkit2gtk::ffi::webkit_get_major_version(),
            webkit2gtk::ffi::webkit_get_minor_version(),
            webkit2gtk::ffi::webkit_get_micro_version()
        )
    }
}

fn read_first_line(path: &Path) -> String {
    std::fs::read_to_string(path).map(|s| s.lines().next().unwrap_or_default().trim().to_string()).unwrap_or_default()
}

pub fn diagnose() -> String {
    let (id, like, pretty) = os_release();
    let media = media_report();
    let gpu = if Path::new("/proc/driver/nvidia/version").exists() {
        let line = read_first_line(Path::new("/proc/driver/nvidia/version"));
        let version = line
            .split_whitespace()
            .find(|w| w.chars().filter(|c| *c == '.').count() >= 1 && w.chars().all(|c| c.is_ascii_digit() || c == '.'))
            .unwrap_or("?")
            .to_string();
        format!("NVIDIA-Treiber {version}")
    } else {
        "kein NVIDIA-Treiber".to_string()
    };
    let crashes = std::fs::read_to_string(crash_log_path())
        .map(|t| t.lines().rev().take(5).collect::<Vec<_>>().join("\n    "))
        .unwrap_or_default();
    let mut out = String::new();
    out.push_str(&format!("PoeziBrowse {}\n", config::VERSION));
    out.push_str(&format!("System:        {pretty} (ID={id}, ID_LIKE={like})\n"));
    out.push_str(&format!("Kernel:        {}\n", read_first_line(Path::new("/proc/sys/kernel/osrelease"))));
    out.push_str(&format!("WebKitGTK:     {}\n", webkit_version()));
    out.push_str(&format!("GTK:           {}.{}.{}\n", gtk::major_version(), gtk::minor_version(), gtk::micro_version()));
    out.push_str(&format!(
        "Sitzung:       {} / {}\n",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_default(),
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
    ));
    out.push_str(&format!("Grafik:        {gpu}\n"));
    out.push_str(&format!("Sandbox:       {}\n", if crate::engine::webkit::sandbox_usable() { "aktiv (bubblewrap)" } else { "aus (bubblewrap fehlt oder darf keine Namespaces anlegen)" }));
    out.push_str(&format!(
        "Darstellung:   {}\n",
        if dmabuf_disabled() { "ohne DMA-BUF (gegen Bildfehler mit NVIDIA unter Wayland)" } else { "DMA-BUF" }
    ));
    out.push_str(&format!("Schriften:     {}\n", if fonts_usable() { "ok".to_string() } else { format!("Schrift-Konfiguration kaputt – Seiten stürzen ab. Reparieren: {}", fonts_install_command()) }));
    out.push_str(&format!("HTTPS (TLS):   {}\n", if tls_available() { "ok".to_string() } else { format!("FEHLT – {}", tls_install_command()) }));
    if media.missing.is_empty() {
        out.push_str("Video/Audio:   alle GStreamer-Plugins vorhanden\n");
    } else {
        out.push_str("Video/Audio:   es fehlen GStreamer-Plugins:\n");
        for (file, purpose) in &media.missing {
            out.push_str(&format!("    {file} – {purpose}\n"));
        }
        out.push_str(&format!("    Installieren: {}\n", media_install_command()));
    }
    out.push_str(&format!(
        "Tor:           {}\n",
        if which("tor") { "installiert" } else { "nicht installiert" }
    ));
    out.push_str(&format!(
        "KeePassXC-CLI: {}\n",
        if which("keepassxc-cli") { "installiert" } else { "nicht installiert" }
    ));
    if !crashes.is_empty() {
        out.push_str(&format!("Letzte Abstürze:\n    {crashes}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_valid_svg() {
        for (name, svg) in ICONS {
            let text = std::str::from_utf8(svg).unwrap();
            assert!(text.contains("<svg"), "{name}");
        }
        assert!(ICONS.iter().any(|(n, _)| *n == "poezi-page-symbolic"));
    }

    #[test]
    fn os_family_detection_does_not_panic() {
        let _ = family();
        let _ = media_install_command();
    }
}
