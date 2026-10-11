use std::collections::HashMap;
use std::fs;

pub fn total_usage() -> u64 {
    let me = std::process::id();
    let parents = parent_map();
    let mut total = 0;
    for (&pid, _) in parents.iter() {
        if pid == me || is_descendant(pid, me, &parents) {
            total += pss_of(pid).unwrap_or(0);
        }
    }
    total
}

pub fn breakdown() -> Vec<(String, u64)> {
    let me = std::process::id();
    let parents = parent_map();
    let (mut ui, mut web, mut net, mut other) = (0, 0, 0, 0);
    let mut web_count = 0;
    for (&pid, _) in parents.iter() {
        if pid != me && !is_descendant(pid, me, &parents) {
            continue;
        }
        let pss = pss_of(pid).unwrap_or(0);
        let name = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        match name.trim() {
            _ if pid == me => ui += pss,
            n if n.starts_with("WebKitWebProces") => {
                web += pss;
                web_count += 1;
            }
            n if n.starts_with("WebKitNetworkPr") => net += pss,
            _ => other += pss,
        }
    }
    vec![
        ("Browser-Oberfläche".into(), ui),
        (format!("Webseiten ({web_count} Prozess{})", if web_count == 1 { "" } else { "e" }), web),
        ("Netzwerk".into(), net),
        ("Sonstige Hilfsprozesse".into(), other),
        ("Gesamt".into(), ui + web + net + other),
    ]
}

fn parent_map() -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else { return map };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        let Some(after) = stat.rfind(')').map(|i| &stat[i + 1..]) else { continue };
        if let Some(ppid) = after.split_whitespace().nth(1).and_then(|s| s.parse().ok()) {
            map.insert(pid, ppid);
        }
    }
    map
}

fn is_descendant(mut pid: u32, ancestor: u32, parents: &HashMap<u32, u32>) -> bool {
    for _ in 0..64 {
        match parents.get(&pid) {
            Some(&parent) if parent == ancestor => return true,
            Some(&parent) if parent > 1 => pid = parent,
            _ => return false,
        }
    }
    false
}

fn pss_of(pid: u32) -> Option<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    let line = text.lines().find(|l| l.starts_with("Pss:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

pub fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.0} MB") }
}

#[cfg(test)]
mod tests {
    #[test]
    fn measures_self() {
        assert!(super::total_usage() > 0);
    }
}
