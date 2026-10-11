use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use keepass::{Database, DatabaseKey};
use zeroize::Zeroizing;

pub const GROUP: &str = "PoeziBrowse";

#[derive(Clone)]
pub struct Location {
    pub path: PathBuf,
    pub keyfile: Option<PathBuf>,
}

pub struct Login {
    pub title: String,
    pub username: String,
    pub url: String,
    pub entry_path: String,
    pub password: Zeroizing<String>,
}

pub fn cli_available() -> bool {
    crate::tor::find_binary(&["keepassxc-cli"]).is_some()
}

pub fn file_state(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((modified, meta.len()))
}

fn key(location: &Location, password: &str) -> Result<DatabaseKey, String> {
    let mut key = DatabaseKey::new().with_password(password);
    if let Some(keyfile) = &location.keyfile {
        let mut file = std::fs::File::open(keyfile).map_err(|e| format!("Schlüsseldatei: {e}"))?;
        key = key.with_keyfile(&mut file).map_err(|e| format!("Schlüsseldatei: {e}"))?;
    }
    Ok(key)
}

pub fn open(location: &Location, password: &str) -> Result<Vec<Login>, String> {
    let bytes = Zeroizing::new(std::fs::read(&location.path).map_err(|e| format!("Datenbank nicht lesbar: {e}"))?);
    let db = Database::parse(&bytes, key(location, password)?).map_err(|e| match e.to_string() {
        m if m.to_lowercase().contains("key") || m.to_lowercase().contains("hmac") => "Falsches Passwort oder falsche Schlüsseldatei.".to_string(),
        m => format!("Datenbank konnte nicht geöffnet werden: {m}"),
    })?;
    let recycle_bin = if db.meta.recyclebin_enabled.unwrap_or(true) { db.meta.recyclebin_uuid } else { None };
    let root_id = db.root().id();
    let parents: std::collections::HashMap<_, _> =
        db.iter_all_groups().map(|g| (g.id(), (g.name.clone(), g.parent().map(|p| p.id())))).collect();
    let mut logins = Vec::new();
    for entry in db.iter_all_entries() {
        let mut groups = Vec::new();
        let mut in_bin = false;
        let mut group = Some(entry.parent().id());
        while let Some(id) = group {
            if Some(id.uuid()) == recycle_bin {
                in_bin = true;
            }
            if id == root_id {
                break;
            }
            let Some((name, parent)) = parents.get(&id) else { break };
            groups.push(name.clone());
            group = *parent;
        }
        if in_bin {
            continue;
        }
        groups.reverse();
        let title = entry.get_title().unwrap_or_default().to_string();
        groups.push(title.clone());
        logins.push(Login {
            title,
            username: entry.get_username().unwrap_or_default().to_string(),
            url: entry.get_url().unwrap_or_default().to_string(),
            entry_path: groups.join("/"),
            password: Zeroizing::new(entry.get_password().unwrap_or_default().to_string()),
        });
    }
    Ok(logins)
}

pub fn entry_host(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let with_scheme = if url.contains("://") { url.to_string() } else { format!("https://{url}") };
    crate::config::host_of(&with_scheme).map(|h| h.trim_start_matches("www.").to_string())
}

pub fn host_matches(entry_url: &str, page_host: &str) -> bool {
    let Some(entry) = entry_host(entry_url) else { return false };
    let page = page_host.trim_start_matches("www.");
    page == entry || page.ends_with(&format!(".{entry}"))
}

pub fn candidate_hosts(page_host: &str) -> Vec<String> {
    let host = page_host.trim_start_matches("www.");
    let labels: Vec<&str> = host.split('.').collect();
    (0..labels.len().saturating_sub(1)).map(|i| labels[i..].join(".")).collect()
}

fn cli(args: &[&str], location: &Location, input: &str) -> Result<String, String> {
    let mut full: Vec<String> = Vec::new();
    full.push(args[0].to_string());
    full.push("-q".into());
    if let Some(keyfile) = &location.keyfile {
        full.push("-k".into());
        full.push(keyfile.to_string_lossy().to_string());
    }
    full.extend(args[1..].iter().map(|a| a.to_string()));
    let mut child = Command::new("keepassxc-cli")
        .args(&full)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("keepassxc-cli nicht startbar: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("keepassxc-cli ohne Eingabe")?;
        let input = Zeroizing::new(input.to_string());
        stdin.write_all(input.as_bytes()).map_err(|e| e.to_string())?;
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn add_login(location: &Location, db_password: &str, host: &str, username: &str, url: &str, password: &str) -> Result<(), String> {
    if !cli_available() {
        return Err("Zum Speichern wird keepassxc-cli benötigt (Paket keepassxc).".into());
    }
    if [host, username, url].iter().any(|v| v.contains(['\n', '\r'])) || password.contains(['\n', '\r']) {
        return Err("Ungültige Zeichen in den Zugangsdaten.".into());
    }
    let db = location.path.to_string_lossy().to_string();
    let _ = cli(&["mkdir", &db, GROUP], location, &format!("{db_password}\n"));
    let existing: Vec<String> = cli(&["ls", &db, GROUP], location, &format!("{db_password}\n"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .collect();
    let mut title = host.to_string();
    if existing.contains(&title) {
        title = format!("{host} ({username})");
    }
    let mut n = 2;
    while existing.contains(&title) {
        title = format!("{host} ({username}) {n}");
        n += 1;
    }
    let path = format!("{GROUP}/{title}");
    cli(&["add", "-u", username, "--url", url, "-p", &db, &path], location, &format!("{db_password}\n{password}\n")).map(|_| ())
}

pub fn update_password(location: &Location, db_password: &str, entry_path: &str, password: &str) -> Result<(), String> {
    if !cli_available() {
        return Err("Zum Speichern wird keepassxc-cli benötigt (Paket keepassxc).".into());
    }
    if password.contains(['\n', '\r']) {
        return Err("Ungültige Zeichen im Passwort.".into());
    }
    let db = location.path.to_string_lossy().to_string();
    cli(&["edit", "-p", &db, entry_path], location, &format!("{db_password}\n{password}\n")).map(|_| ())
}

pub fn create_database(location: &Location, db_password: &str) -> Result<(), String> {
    if let Some(parent) = location.path.parent() {
        let _ = crate::config::private_dir(parent);
    }
    if cli_available() {
        let db = location.path.to_string_lossy().to_string();
        let mut child = Command::new("keepassxc-cli")
            .args(["db-create", "-q", "-p", &db])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        {
            let mut stdin = child.stdin.take().ok_or("keine Eingabe")?;
            let input = Zeroizing::new(format!("{db_password}\n{db_password}\n"));
            stdin.write_all(input.as_bytes()).map_err(|e| e.to_string())?;
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        return if output.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&output.stderr).trim().to_string()) };
    }
    let db = Database::new();
    let mut file = std::fs::File::create(&location.path).map_err(|e| e.to_string())?;
    db.save(&mut file, DatabaseKey::new().with_password(db_password)).map_err(|e| e.to_string())
}

pub fn generate_password() -> Zeroizing<String> {
    if cli_available() {
        if let Ok(output) = Command::new("keepassxc-cli")
            .args(["generate", "-q", "-L", "20", "-l", "-U", "-n", "-s", "--every-group", "--exclude-similar"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        {
            let text = Zeroizing::new(String::from_utf8_lossy(&output.stdout).trim().to_string());
            if output.status.success() && text.chars().count() >= 16 {
                return text;
            }
        }
    }
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!#$%&()*+,-./:;<=>?@[]^_{}~";
    let mut out = Zeroizing::new(String::new());
    while out.len() < 20 {
        let mut byte = [0u8; 1];
        if getrandom::fill(&mut byte).is_err() {
            break;
        }
        let limit = 256 - (256 % CHARS.len());
        if (byte[0] as usize) < limit {
            out.push(CHARS[byte[0] as usize % CHARS.len()] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        assert!(host_matches("https://github.com/login", "github.com"));
        assert!(host_matches("github.com", "www.github.com"));
        assert!(host_matches("https://google.com", "accounts.google.com"));
        assert!(!host_matches("https://github.com", "evilgithub.com"));
        assert!(!host_matches("https://github.com", "github.com.evil.org"));
        assert!(!host_matches("", "github.com"));
    }

    #[test]
    fn candidates() {
        assert_eq!(candidate_hosts("accounts.google.com"), vec!["accounts.google.com", "google.com"]);
        assert_eq!(candidate_hosts("www.github.com"), vec!["github.com"]);
    }

    #[test]
    fn generated() {
        let a = generate_password();
        let b = generate_password();
        assert!(a.chars().count() >= 16);
        assert_ne!(*a, *b);
    }
}
