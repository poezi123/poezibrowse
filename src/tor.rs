use std::cell::RefCell;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::glib;

use crate::config::{self, Config};

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
    pub package: &'static str,
}

pub const TRANSPORTS: &[Transport] = &[
    Transport { name: "obfs4", binaries: &["lyrebird", "obfs4proxy"], package: "lyrebird-proxy" },
    Transport { name: "webtunnel", binaries: &["lyrebird", "webtunnel-client"], package: "lyrebird-proxy" },
    Transport { name: "meek_lite", binaries: &["lyrebird", "obfs4proxy"], package: "lyrebird-proxy" },
    Transport { name: "snowflake", binaries: &["snowflake-client"], package: "snowflake-pt-client-git" },
];

pub fn find_binary(names: &[&str]) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    names
        .iter()
        .flat_map(|name| std::env::split_paths(&paths).map(move |dir| dir.join(name)))
        .find(|p| p.is_file())
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

pub fn torrc(config: &Config, port: u16, data_dir: &std::path::Path) -> Result<String, String> {
    let mut rc = format!(
        "DataDirectory {}\nSocksPort 127.0.0.1:{port}\nLog notice stdout\nAvoidDiskWrites 1\nClientOnly 1\n__OwningControllerProcess {}\n",
        data_dir.display(),
        std::process::id()
    );
    if config.tor_use_bridges {
        let bridges = clean_bridges(&config.tor_bridges);
        if bridges.is_empty() {
            return Err("Brücken sind eingeschaltet, aber es ist keine Brücke eingetragen.".into());
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
            let binary = find_binary(transport.binaries).ok_or_else(|| {
                format!("Für {name}-Brücken fehlt das Programm {} (Paket {}).", transport.binaries[0], transport.package)
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
}

impl Tor {
    pub fn new() -> Rc<Tor> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .unwrap_or(19150);
        Rc::new(Tor { port, child: RefCell::new(None), status: RefCell::new(Status::default()), listeners: RefCell::new(Vec::new()) })
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
            self.set_status(fail("Tor ist nicht installiert (Paket „tor“).".into()));
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
                me.set_status(status);
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn stop(&self) {
        if let Some(mut child) = self.child.borrow_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.set_status(Status::default());
    }
}

impl Drop for Tor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.borrow_mut().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
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
