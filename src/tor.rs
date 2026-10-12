use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::glib;

use crate::config::{self, BridgeKind, Config};

#[derive(Clone, Default)]
pub struct Status {
    pub running: bool,
    pub progress: u8,
    pub summary: String,
    pub error: Option<String>,
}

impl Status {
    pub fn ready(&self) -> bool {
        self.running && self.progress >= 100
    }

    pub fn label(&self) -> String {
        if let Some(error) = &self.error {
            return format!("Fehler: {error}");
        }
        if self.ready() {
            "Verbunden".into()
        } else if self.running {
            format!("Verbinde … {} %", self.progress)
        } else {
            "Aus".into()
        }
    }
}

pub struct Transport {
    pub name: &'static str,
    pub binaries: &'static [&'static str],
}

pub const TRANSPORTS: &[Transport] = &[
    Transport { name: "obfs4", binaries: &["lyrebird", "obfs4proxy"] },
    Transport { name: "webtunnel", binaries: &["lyrebird", "webtunnel-client"] },
    Transport { name: "meek_lite", binaries: &["lyrebird", "obfs4proxy"] },
    Transport { name: "snowflake", binaries: &["lyrebird", "snowflake-client"] },
];

pub fn find_binary(names: &[&str]) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    names
        .iter()
        .flat_map(|name| std::env::split_paths(&paths).map(move |dir| dir.join(name)))
        .find(|p| p.is_file())
}

pub fn bundled_lyrebird() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let prefix = exe.parent()?.parent()?;
    ["lib/poezibrowse/lyrebird", "lib64/poezibrowse/lyrebird", "libexec/poezibrowse/lyrebird"]
        .iter()
        .map(|p| prefix.join(p))
        .find(|p| p.is_file())
}

pub fn transport_binary(transport: &Transport) -> Option<PathBuf> {
    bundled_lyrebird().or_else(|| find_binary(transport.binaries))
}

const BUILTIN: &str = include_str!("../assets/tor/bridges.json");

pub fn builtin_bridges(kind: BridgeKind) -> Vec<String> {
    let key = match kind {
        BridgeKind::Obfs4 => "obfs4",
        BridgeKind::Snowflake => "snowflake",
        BridgeKind::Meek => "meek",
        BridgeKind::Custom => return Vec::new(),
    };
    serde_json::from_str::<serde_json::Value>(BUILTIN)
        .ok()
        .and_then(|v| v.get("bridges")?.get(key)?.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|l| l.as_str().map(String::from))
        .collect()
}

fn bridge_transport(line: &str) -> Option<&str> {
    let first = line.split_whitespace().next()?;
    let is_address = first.contains(':') && first.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '[');
    (!is_address).then_some(first)
}

pub fn clean_bridges(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.trim().trim_start_matches("Bridge ").trim_start_matches("bridge ").trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.contains(['\n', '\r']))
        .collect()
}

fn control_dir() -> PathBuf {
    dirs::runtime_dir()
        .map(|d| d.join(format!("poezibrowse-{}", std::process::id())))
        .unwrap_or_else(|| config::data_dir().join("tor"))
}

fn control_socket() -> PathBuf {
    control_dir().join("control")
}

fn control_cookie() -> PathBuf {
    control_dir().join("control.cookie")
}

pub fn torrc(config: &Config, port: u16, data_dir: &std::path::Path) -> Result<String, String> {
    let mut rc = format!(
        "DataDirectory \"{}\"\nSocksPort 127.0.0.1:{port}\nLog notice stdout\nAvoidDiskWrites 1\nClientOnly 1\n__OwningControllerProcess {}\n\
         ControlSocket \"{}\"\nCookieAuthentication 1\nCookieAuthFile \"{}\"\n",
        data_dir.display(),
        std::process::id(),
        control_socket().display(),
        control_cookie().display()
    );
    if let Some(kind) = config.bridge_kind() {
        let bridges = match kind {
            BridgeKind::Custom => clean_bridges(&config.tor_bridges),
            builtin => builtin_bridges(builtin),
        };
        if bridges.is_empty() {
            return Err("Eigene Brücken sind ausgewählt, aber es ist keine Brücke eingetragen.".into());
        }
        rc.push_str("UseBridges 1\n");
        let mut needed: Vec<&str> = bridges.iter().filter_map(|b| bridge_transport(b)).collect();
        needed.sort_unstable();
        needed.dedup();
        for name in needed {
            let transport = TRANSPORTS
                .iter()
                .find(|t| t.name == name)
                .ok_or_else(|| format!("Die Brückenart „{name}“ wird nicht unterstützt."))?;
            let binary = transport_binary(transport).ok_or_else(|| {
                format!("Für {name}-Brücken fehlt das Programm lyrebird. Es ist in den PoeziBrowse-Paketen enthalten – bitte PoeziBrowse neu installieren.")
            })?;
            rc.push_str(&format!("ClientTransportPlugin {name} exec {}\n", binary.display()));
        }
        for bridge in bridges {
            rc.push_str(&format!("Bridge {bridge}\n"));
        }
    }
    Ok(rc)
}

pub struct Tor {
    pub port: u16,
    child: RefCell<Option<Child>>,
    status: RefCell<Status>,
    listeners: RefCell<Vec<Box<dyn Fn(&Status)>>>,
    watching: std::cell::Cell<bool>,
}

impl Tor {
    pub fn new() -> Rc<Tor> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .unwrap_or(19150);
        Rc::new(Tor { port, child: RefCell::new(None), status: RefCell::new(Status::default()), listeners: RefCell::new(Vec::new()), watching: std::cell::Cell::new(false) })
    }

    pub fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    pub fn connect_changed(&self, f: impl Fn(&Status) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    fn set_status(&self, status: Status) {
        *self.status.borrow_mut() = status.clone();
        for f in self.listeners.borrow().iter() {
            f(&status);
        }
    }

    pub fn is_running(&self) -> bool {
        self.child.borrow().is_some()
    }

    pub fn start(self: &Rc<Self>, config: &Config) {
        if self.is_running() {
            return;
        }
        let fail = |message: String| Status { error: Some(message), ..Status::default() };
        let Some(binary) = find_binary(&["tor"]) else {
            self.set_status(fail(format!("Tor ist nicht installiert – {}", crate::system::tor_install_command())));
            return;
        };
        let dir = config::data_dir().join("tor");
        let data = dir.join("data");
        if let Err(e) = config::private_dir(&data) {
            self.set_status(fail(e.to_string()));
            return;
        }
        let rc = match torrc(config, self.port, &data) {
            Ok(rc) => rc,
            Err(e) => {
                self.set_status(fail(e));
                return;
            }
        };
        if let Err(e) = config::private_dir(&control_dir()) {
            self.set_status(fail(e.to_string()));
            return;
        }
        let rc_path = dir.join("torrc");
        if let Err(e) = config::write_private(&rc_path, rc.as_bytes()) {
            self.set_status(fail(e.to_string()));
            return;
        }
        let child = Command::new(binary)
            .arg("-f")
            .arg(&rc_path)
            .arg("--defaults-torrc")
            .arg("/dev/null")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                self.set_status(fail(format!("Tor konnte nicht gestartet werden: {e}")));
                return;
            }
        };
        let (tx, rx) = mpsc::channel::<String>();
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        *self.child.borrow_mut() = Some(child);
        self.set_status(Status { running: true, summary: "Starte Tor".into(), ..Status::default() });

        let me = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
            let mut status = me.status();
            let mut changed = false;
            while let Ok(line) = rx.try_recv() {
                if let Some(rest) = line.split("Bootstrapped ").nth(1) {
                    if let Some(percent) = rest.split('%').next().and_then(|p| p.trim().parse::<u8>().ok()) {
                        status.progress = percent;
                        status.summary = rest.split_once("): ").map(|(_, s)| s.to_string()).unwrap_or_default();
                        status.error = None;
                        changed = true;
                    }
                } else if line.contains("[err]") {
                    status.error = Some(line.split("[err] ").nth(1).unwrap_or(&line).to_string());
                    changed = true;
                }
            }
            let exited = me.child.borrow_mut().as_mut().is_none_or(|c| c.try_wait().ok().flatten().is_some());
            if exited {
                me.child.borrow_mut().take();
                if status.error.is_none() && status.running {
                    status.error = Some("Tor wurde beendet.".into());
                }
                status.running = false;
                me.set_status(status);
                return glib::ControlFlow::Break;
            }
            if changed {
                if status.ready() && !me.watching.replace(true) {
                    watch_streams();
                }
                me.set_status(status);
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn stop(&self) {
        self.watching.set(false);
        if let Some(mut child) = self.child.borrow_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(control_dir());
        self.set_status(Status::default());
    }
}

impl Drop for Tor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.borrow_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(control_dir());
    }
}

pub struct Relay {
    pub role: String,
    pub fingerprint: String,
    pub ip: Option<String>,
    pub country: Option<String>,
}

pub fn flag(country: &str) -> String {
    country
        .to_ascii_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .filter_map(|c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)))
        .collect()
}

struct Control {
    reader: BufReader<std::os::unix::net::UnixStream>,
    writer: std::os::unix::net::UnixStream,
}

impl Control {
    fn connect() -> Result<Control, String> {
        let stream = std::os::unix::net::UnixStream::connect(control_socket()).map_err(|e| format!("Tor-Steuerung nicht erreichbar: {e}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut control = Control { reader: BufReader::new(stream), writer };
        let cookie = std::fs::read(control_cookie()).map_err(|e| e.to_string())?;
        let hex: String = cookie.iter().map(|b| format!("{b:02X}")).collect();
        control.command(&format!("AUTHENTICATE {hex}"))?;
        Ok(control)
    }

    fn command(&mut self, command: &str) -> Result<Vec<String>, String> {
        use std::io::Write;
        if command.contains(['\r', '\n']) {
            return Err("ungültiger Befehl".into());
        }
        self.writer.write_all(format!("{command}\r\n").as_bytes()).map_err(|e| e.to_string())?;
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                return Err("Tor hat die Verbindung geschlossen".into());
            }
            let line = line.trim_end_matches(['\r', '\n']).to_string();
            let is_final = line.len() >= 4 && line.as_bytes()[3] == b' ' && line[..3].chars().all(|c| c.is_ascii_digit());
            if is_final {
                if !line.starts_with('2') {
                    return Err(line);
                }
                return Ok(lines);
            }
            lines.push(line);
        }
    }

    fn info(&mut self, key: &str) -> Result<Vec<String>, String> {
        let lines = self.command(&format!("GETINFO {key}"))?;
        let mut values = Vec::new();
        for line in lines {
            if let Some(rest) = line.strip_prefix(&format!("250-{key}=")) {
                if !rest.is_empty() {
                    values.push(rest.to_string());
                }
            } else if line.starts_with(&format!("250+{key}=")) || line == "." {
                continue;
            } else {
                values.push(line);
            }
        }
        Ok(values)
    }
}

static STREAM_CIRCUITS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

fn remember_stream(host: &str, circuit: &str) {
    if let Ok(mut map) = STREAM_CIRCUITS.lock() {
        map.retain(|(h, _)| h != host);
        map.push((host.to_string(), circuit.to_string()));
        if map.len() > 300 {
            map.remove(0);
        }
    }
}

fn remembered_circuit(host: &str) -> Option<String> {
    STREAM_CIRCUITS.lock().ok()?.iter().rev().find(|(h, _)| h == host).map(|(_, c)| c.clone())
}

fn parse_stream_event(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.first() != Some(&"650") || parts.get(1) != Some(&"STREAM") {
        return None;
    }
    if !matches!(parts.get(3), Some(&"SENTCONNECT") | Some(&"SUCCEEDED")) {
        return None;
    }
    let circuit = parts.get(4).filter(|c| **c != "0")?;
    let target = parts.get(5)?;
    let host = target.rsplit_once(':').map_or(*target, |(h, _)| h);
    Some((host.to_string(), circuit.to_string()))
}

fn watch_streams() {
    std::thread::spawn(|| {
        let Ok(mut control) = Control::connect() else { return };
        if control.command("SETEVENTS STREAM").is_err() {
            return;
        }
        loop {
            let mut line = String::new();
            match control.reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Some((host, circuit)) = parse_stream_event(line.trim_end()) {
                        remember_stream(&host, &circuit);
                    }
                }
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
                Err(_) => break,
            }
        }
    });
}

pub fn circuit_for(host: &str) -> Result<Vec<Relay>, String> {
    let mut control = Control::connect()?;
    let streams = control.info("stream-status")?;
    let wanted = streams.iter().rev().find_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let target = parts.get(3)?;
        let target_host = target.rsplit_once(':').map_or(*target, |(h, _)| h);
        (target_host == host).then(|| parts.get(2).map(|c| c.to_string())).flatten()
    });
    let wanted = wanted.or_else(|| remembered_circuit(host));
    let circuits = control.info("circuit-status")?;
    let built: Vec<Vec<&str>> = circuits.iter().map(|l| l.split_whitespace().collect::<Vec<_>>()).filter(|p| p.get(1) == Some(&"BUILT")).collect();
    let circuit = wanted
        .and_then(|id| built.iter().find(|p| p[0] == id))
        .ok_or("Für diese Seite gibt es gerade keine offene Tor-Verbindung. Lade die Seite neu und öffne die Anzeige dann noch einmal.")?;
    let path: Vec<String> = circuit
        .get(2)
        .unwrap_or(&"")
        .split(',')
        .filter(|h| !h.is_empty())
        .map(|hop| {
            let hop = hop.trim_start_matches('$');
            hop.split_once(['~', '=']).map_or(hop, |(fp, _)| fp).to_string()
        })
        .collect();
    let count = path.len();
    let mut relays = Vec::new();
    for (i, fingerprint) in path.into_iter().enumerate() {
        let role = match (i, count) {
            (0, _) => "Eingang (Guard)".to_string(),
            (i, 3) if i == 2 => "Ausgang (Exit)".to_string(),
            (i, n) if i == n - 1 => "Rendezvous (.onion)".to_string(),
            _ => "Mitte".to_string(),
        };
        let ip = control
            .info(&format!("ns/id/{fingerprint}"))
            .ok()
            .and_then(|lines| lines.into_iter().find(|l| l.starts_with("r ")))
            .and_then(|r| r.split_whitespace().nth(6).map(String::from));
        let country = ip.as_ref().and_then(|ip| {
            control
                .info(&format!("ip-to-country/{ip}"))
                .ok()?
                .into_iter()
                .next()
                .filter(|c| c.len() == 2 && c != "??")
        });
        relays.push(Relay { role, fingerprint, ip, country });
    }
    Ok(relays)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_lines() {
        let lines = vec![
            "Bridge obfs4 1.2.3.4:443 FINGERPRINT cert=abc iat-mode=0".to_string(),
            "  ".to_string(),
            "# comment".to_string(),
            "5.6.7.8:9001 ABCDEF".to_string(),
        ];
        let clean = clean_bridges(&lines);
        assert_eq!(clean.len(), 2);
        assert_eq!(bridge_transport(&clean[0]), Some("obfs4"));
        assert_eq!(bridge_transport(&clean[1]), None);
        assert_eq!(bridge_transport("[2001:db8::1]:443 FP"), None);
    }

    #[test]
    fn builtin() {
        for kind in [BridgeKind::Obfs4, BridgeKind::Snowflake, BridgeKind::Meek] {
            let bridges = builtin_bridges(kind);
            assert!(!bridges.is_empty());
            assert!(clean_bridges(&bridges).len() == bridges.len());
        }
        assert!(builtin_bridges(BridgeKind::Obfs4).iter().all(|b| b.starts_with("obfs4 ")));
        assert!(builtin_bridges(BridgeKind::Custom).is_empty());
    }

    #[test]
    fn bridge_kind_migration() {
        let off = Config::default();
        assert_eq!(off.bridge_kind(), None);
        let old = Config { tor_use_bridges: true, tor_bridges: vec!["obfs4 1.2.3.4:1 FP cert=x iat-mode=0".into()], ..Config::default() };
        assert_eq!(old.bridge_kind(), Some(BridgeKind::Custom));
        let fresh = Config { tor_use_bridges: true, ..Config::default() };
        assert_eq!(fresh.bridge_kind(), Some(BridgeKind::Obfs4));
    }

    #[test]
    fn stream_events() {
        assert_eq!(
            parse_stream_event("650 STREAM 42 SUCCEEDED 17 check.torproject.org:443 SOURCE_ADDR=127.0.0.1:5555 PURPOSE=USER"),
            Some(("check.torproject.org".into(), "17".into()))
        );
        assert_eq!(parse_stream_event("650 STREAM 42 NEW 0 example.com:443"), None);
        assert_eq!(parse_stream_event("650 CIRC 17 BUILT"), None);
    }

    #[test]
    fn flags() {
        assert_eq!(flag("de"), "🇩🇪");
        assert_eq!(flag("??"), "");
    }

    #[test]
    fn torrc_without_bridges() {
        let rc = torrc(&Config::default(), 9999, std::path::Path::new("/tmp/x")).unwrap();
        assert!(rc.contains("SocksPort 127.0.0.1:9999"));
        assert!(!rc.contains("UseBridges"));
    }

    #[test]
    fn torrc_rejects_injection() {
        let c = Config { tor_use_bridges: true, tor_bridges: vec!["1.2.3.4:1 FP\nSocksPort 0.0.0.0:9050".into()], ..Config::default() };
        let rc = torrc(&c, 1, std::path::Path::new("/tmp/x"));
        assert!(rc.is_err() || !rc.unwrap().contains("0.0.0.0"));
    }
}
