use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zeroize::{Zeroize, Zeroizing};

use crate::config;

pub const LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Store {
    Tpm,
    Keyring,
    Memory,
}

impl Store {
    pub fn describe(self) -> &'static str {
        match self {
            Store::Tpm => "verschlüsselt mit dem TPM-Chip deines PCs (systemd-creds), nur für deinen Benutzer, läuft nach 7 Tagen ab",
            Store::Keyring => "im Schlüsselbund des Systems (KWallet), läuft nach 7 Tagen ab",
            Store::Memory => "nur im Arbeitsspeicher, bis PoeziBrowse geschlossen wird",
        }
    }
}

#[derive(Clone, Copy)]
pub enum Kind {
    Master,
    IndexKey,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Master => "poezibrowse-master",
            Kind::IndexKey => "poezibrowse-index-key",
        }
    }

    fn file(self) -> PathBuf {
        dir().join(format!("{}.cred", self.name()))
    }

    fn expires(self) -> bool {
        matches!(self, Kind::Master)
    }
}

pub fn dir() -> PathBuf {
    config::data_dir().join("passwords")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn run_with_input(program: &str, args: &[&str], input: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    {
        let mut stdin = child.stdin.take()?;
        stdin.write_all(input).ok()?;
    }
    let mut output = child.wait_with_output().ok()?;
    let result = Zeroizing::new(std::mem::take(&mut output.stdout));
    output.stderr.zeroize();
    output.status.success().then_some(result)
}

fn into_string(bytes: Zeroizing<Vec<u8>>) -> Option<Zeroizing<String>> {
    let text = std::str::from_utf8(&bytes).ok()?;
    Some(Zeroizing::new(text.to_string()))
}

pub fn store(kind: Kind, value: &str) -> Store {
    let _ = config::private_dir(&dir());
    let file = kind.file();
    let file_arg = file.to_string_lossy().to_string();
    let mut args = vec!["--user", "encrypt", "--name", kind.name()];
    if kind.expires() {
        args.push("--not-after=+7d");
    }
    args.push("-");
    args.push(&file_arg);
    if run_with_input("systemd-creds", &args, value.as_bytes()).is_some() && file.is_file() {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        return Store::Tpm;
    }
    let created = now().to_string();
    let stored = run_with_input(
        "secret-tool",
        &["store", "--label", "PoeziBrowse", "application", "poezibrowse", "kind", kind.name(), "created", &created],
        value.as_bytes(),
    );
    if stored.is_some() { Store::Keyring } else { Store::Memory }
}

pub fn load(kind: Kind) -> Option<Zeroizing<String>> {
    let file = kind.file();
    if file.is_file() {
        let file_arg = file.to_string_lossy().to_string();
        match run_with_input("systemd-creds", &["--user", "decrypt", "--name", kind.name(), &file_arg, "-"], b"") {
            Some(bytes) => return into_string(bytes),
            None => {
                let _ = std::fs::remove_file(&file);
                return None;
            }
        }
    }
    let created = Command::new("secret-tool")
        .args(["search", "--all", "application", "poezibrowse", "kind", kind.name()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find_map(|l| l.strip_prefix("attribute.created = ").and_then(|v| v.trim().parse::<u64>().ok()))
        })?;
    if kind.expires() && now().saturating_sub(created) > LIFETIME.as_secs() {
        forget(kind);
        return None;
    }
    into_string(run_with_input("secret-tool", &["lookup", "application", "poezibrowse", "kind", kind.name()], b"")?)
}

pub fn forget(kind: Kind) {
    let _ = std::fs::remove_file(kind.file());
    let _ = Command::new("secret-tool")
        .args(["clear", "application", "poezibrowse", "kind", kind.name()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub struct Cache {
    entries: RefCell<Vec<(&'static str, Zeroizing<String>, Instant)>>,
}

impl Cache {
    pub fn new() -> Cache {
        Cache { entries: RefCell::new(Vec::new()) }
    }

    pub fn put(&self, kind: Kind, value: &str) {
        let mut entries = self.entries.borrow_mut();
        entries.retain(|(name, _, _)| *name != kind.name());
        entries.push((kind.name(), Zeroizing::new(value.to_string()), Instant::now()));
    }

    pub fn get(&self, kind: Kind) -> Option<Zeroizing<String>> {
        self.entries
            .borrow()
            .iter()
            .find(|(name, _, at)| *name == kind.name() && (!kind.expires() || at.elapsed() < LIFETIME))
            .map(|(_, value, _)| value.clone())
    }

    pub fn clear(&self, kind: Kind) {
        self.entries.borrow_mut().retain(|(name, _, _)| *name != kind.name());
    }
}
