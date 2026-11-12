use crate::procs::now_ms;
use crate::util::format_rfc3339;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

const EVERY_MS: i64 = 2000;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const RECHECK_MS: i64 = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Listen {
    pub pid: u32,
    pub process: String,
    pub addr: String,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub port: u16,
    pub addr: String,
    pub pid: u32,
    pub process: String,
    pub term_id: String,
    pub line_id: Option<String>,
    pub project: String,
    pub since_ms: i64,
}

impl Port {
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "port": self.port, "addr": self.addr, "pid": self.pid,
            "process": self.process, "termId": self.term_id,
        });
        if let Some(l) = &self.line_id {
            v["lineId"] = json!(l);
        }
        v["project"] = json!(self.project);
        v["since"] = json!(format_rfc3339(self.since_ms));
        v
    }
}

#[derive(Clone, Debug)]
pub struct Root {
    pub pid: u32,
    pub term_id: String,
    pub line_id: Option<String>,
    pub cwd: String,
}

fn v4(word: &str) -> Option<Ipv4Addr> {
    Some(Ipv4Addr::from(
        u32::from_str_radix(word, 16).ok()?.to_le_bytes(),
    ))
}

fn proc_addr(hex: &str) -> Option<String> {
    match hex.len() {
        8 => Some(v4(hex)?.to_string()),
        32 => {
            let mut b = [0u8; 16];
            for i in 0..4 {
                let w = u32::from_str_radix(hex.get(i * 8..i * 8 + 8)?, 16).ok()?;
                b[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
            let a = Ipv6Addr::from(b);
            Some(match a.to_ipv4_mapped() {
                Some(m) => m.to_string(),
                None => a.to_string(),
            })
        }
        _ => None,
    }
}

pub fn parse_proc_net(text: &str) -> Vec<(u64, String, u16)> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 10 || f[3] != "0A" {
                return None;
            }
            let (a, p) = f[1].split_once(':')?;
            let port = u16::from_str_radix(p, 16).ok()?;
            let inode = f[9].parse().ok().filter(|i| *i != 0)?;
            Some((inode, proc_addr(a)?, port))
        })
        .collect()
}

pub fn parse_lsof(text: &str) -> Vec<Listen> {
    let (mut pid, mut cmd) = (0u32, String::new());
    let mut out = vec![];
    for l in text.lines() {
        let Some(tag) = l.chars().next() else {
            continue;
        };
        let rest = &l[tag.len_utf8()..];
        match tag {
            'p' => {
                pid = rest.parse().unwrap_or(0);
                cmd.clear();
            }
            'c' => cmd = rest.to_string(),
            'n' if pid != 0 => {
                let Some((a, p)) = rest.rsplit_once(':') else {
                    continue;
                };
                let Ok(port) = p.parse::<u16>() else {
                    continue;
                };
                let a = a.trim_start_matches('[').trim_end_matches(']');
                let addr = match a {
                    "*" | "" => "0.0.0.0".to_string(),
                    _ => a.to_string(),
                };
                out.push(Listen {
                    pid,
                    process: cmd.clone(),
                    addr,
                    port,
                });
            }
            _ => {}
        }
    }
    out
}

fn width(addr: &str) -> u8 {
    match addr {
        "0.0.0.0" | "::" => 2,
        a if a == "::1" || a.starts_with("127.") => 0,
        _ => 1,
    }
}

pub fn widest(ls: Vec<Listen>) -> Vec<Listen> {
    let mut by: BTreeMap<u16, Listen> = BTreeMap::new();
    for l in ls {
        match by.get(&l.port) {
            Some(o) if (width(&o.addr), o.pid) >= (width(&l.addr), l.pid) => {}
            _ => {
                by.insert(l.port, l);
            }
        }
    }
    by.into_values().collect()
}

pub fn diff(old: &BTreeMap<u16, Port>, new: &BTreeMap<u16, Port>) -> (Vec<Port>, Vec<Port>) {
    let same = |a: &Port, b: &Port| a.pid == b.pid && a.addr == b.addr && a.term_id == b.term_id;
    let opened = new
        .values()
        .filter(|n| !old.get(&n.port).is_some_and(|o| same(o, n)))
        .cloned()
        .collect();
    let closed = old
        .values()
        .filter(|o| !new.get(&o.port).is_some_and(|n| same(o, n)))
        .cloned()
        .collect();
    (opened, closed)
}

fn project(cwd: &str) -> String {
    let mut d = Path::new(cwd);
    loop {
        if d.join(".git").exists() {
            return d.to_string_lossy().into_owned();
        }
        match d.parent() {
            Some(p) => d = p,
            None => return cwd.to_string(),
        }
    }
}

fn ppids() -> Vec<(u32, u32)> {
    if cfg!(target_os = "linux") {
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return vec![];
        };
        dir.flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
            .filter_map(|pid| {
                let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
                let after = &stat[stat.rfind(')')? + 1..];
                Some((pid, after.split_whitespace().nth(1)?.parse().ok()?))
            })
            .collect()
    } else {
        crate::exec::output_within(
            std::process::Command::new("ps").args(["-axo", "pid=,ppid="]),
            crate::exec::PROBE_TIMEOUT,
        )
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect()
    }
}

pub fn owners(roots: &[Root], ppids: &[(u32, u32)]) -> HashMap<u32, usize> {
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, ppid) in ppids {
        if pid != ppid {
            kids.entry(ppid).or_default().push(pid);
        }
    }
    let mut own = HashMap::new();
    for (i, r) in roots.iter().enumerate() {
        let mut stack = vec![r.pid];
        while let Some(p) = stack.pop() {
            if own.insert(p, i).is_some() {
                continue;
            }
            stack.extend(kids.get(&p).into_iter().flatten().copied());
        }
    }
    own
}

#[cfg(target_os = "linux")]
fn has_children_files() -> bool {
    static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OK.get_or_init(|| Path::new("/proc/thread-self/children").exists())
}

#[cfg(target_os = "linux")]
fn kids(pid: u32) -> Vec<u32> {
    let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return vec![];
    };
    dir.flatten()
        .filter_map(|t| std::fs::read_to_string(t.path().join("children")).ok())
        .flat_map(|s| {
            s.split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect::<Vec<u32>>()
        })
        .collect()
}

fn owned(roots: &[Root]) -> HashMap<u32, usize> {
    #[cfg(target_os = "linux")]
    if has_children_files() {
        let mut own = HashMap::new();
        for (i, r) in roots.iter().enumerate() {
            let mut stack = vec![r.pid];
            while let Some(p) = stack.pop() {
                if own.insert(p, i).is_some() {
                    continue;
                }
                stack.extend(kids(p));
            }
        }
        return own;
    }
    #[cfg(target_os = "macos")]
    {
        let mut own = HashMap::new();
        for (i, r) in roots.iter().enumerate() {
            let mut stack = vec![r.pid];
            while let Some(p) = stack.pop() {
                if own.insert(p, i).is_some() {
                    continue;
                }
                stack.extend(crate::procs::mac::children(p));
            }
        }
        return own;
    }
    #[allow(unreachable_code)]
    owners(roots, &ppids())
}

fn fingerprint(roots: &[Root], socks: &impl std::hash::Hash) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in roots {
        (r.pid, &r.term_id, &r.line_id, &r.cwd).hash(&mut h);
    }
    socks.hash(&mut h);
    h.finish()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn unchanged(last: &mut Option<(u64, i64)>, key: u64, now: i64) -> bool {
    if matches!(*last, Some((k, at)) if k == key && now.saturating_sub(at) < RECHECK_MS) {
        return true;
    }
    *last = Some((key, now));
    false
}

#[cfg(target_os = "linux")]
fn sockets(pids: &[u32]) -> Vec<(u32, u64)> {
    let mut out = vec![];
    for &pid in pids {
        let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for e in dir.flatten() {
            let Ok(link) = std::fs::read_link(e.path()) else {
                continue;
            };
            if let Some(inode) = link
                .to_str()
                .and_then(|s| s.strip_prefix("socket:["))
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<u64>().ok())
            {
                out.push((pid, inode));
            }
        }
    }
    out.sort_unstable();
    out
}

#[cfg(target_os = "linux")]
fn listening() -> HashMap<u64, (String, u16)> {
    let mut socks = HashMap::new();
    for f in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        for (inode, addr, port) in parse_proc_net(&text) {
            socks.insert(inode, (addr, port));
        }
    }
    socks
}

#[cfg(target_os = "linux")]
fn resolve(listen: &HashMap<u64, (String, u16)>, socks: &[(u32, u64)]) -> Vec<Listen> {
    let mut comm: HashMap<u32, String> = HashMap::new();
    socks
        .iter()
        .filter_map(|(pid, inode)| {
            let (addr, port) = listen.get(inode)?;
            let process = comm
                .entry(*pid)
                .or_insert_with(|| {
                    std::fs::read_to_string(format!("/proc/{pid}/comm"))
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                })
                .clone();
            Some(Listen {
                pid: *pid,
                process,
                addr: addr.clone(),
                port: *port,
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn listeners(pids: &[u32]) -> Vec<Listen> {
    let list: Vec<String> = pids.iter().map(u32::to_string).collect();
    crate::exec::output_within(
        std::process::Command::new("lsof").args([
            "-nP",
            "-a",
            "-iTCP",
            "-sTCP:LISTEN",
            "-p",
            &list.join(","),
            "-F",
            "pcn",
        ]),
        crate::exec::PROBE_TIMEOUT,
    )
    .map(|o| parse_lsof(&String::from_utf8_lossy(&o.stdout)))
    .unwrap_or_default()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn listeners(_pids: &[u32]) -> Vec<Listen> {
    vec![]
}

pub fn scan(roots: &[Root], now: i64) -> BTreeMap<u16, Port> {
    scan_since(roots, now, &mut None).unwrap_or_default()
}

fn scan_since(
    roots: &[Root],
    now: i64,
    last: &mut Option<(u64, i64)>,
) -> Option<BTreeMap<u16, Port>> {
    if roots.is_empty() {
        *last = None;
        return Some(BTreeMap::new());
    }
    let own = owned(roots);
    let mut pids: Vec<u32> = own.keys().copied().collect();
    pids.sort_unstable();
    #[cfg(target_os = "linux")]
    let found = {
        let socks = sockets(&pids);
        if unchanged(last, fingerprint(roots, &socks), now) {
            return None;
        }
        if socks.is_empty() {
            vec![]
        } else {
            resolve(&listening(), &socks)
        }
    };
    #[cfg(target_os = "macos")]
    let found = {
        let socks: Vec<(u32, Vec<i32>)> = pids
            .iter()
            .filter_map(|p| Some((*p, crate::procs::mac::socket_fds(*p)?)))
            .filter(|(_, fds)| !fds.is_empty())
            .collect();
        if unchanged(last, fingerprint(roots, &socks), now) {
            return None;
        }
        let with: Vec<u32> = socks.iter().map(|s| s.0).collect();
        if with.is_empty() {
            vec![]
        } else {
            listeners(&with)
        }
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let found = {
        let _ = (&last, fingerprint(roots, &()));
        listeners(&pids)
    };
    let ports = widest(found)
        .into_iter()
        .filter_map(|l| {
            let r = &roots[*own.get(&l.pid)?];
            Some((
                l.port,
                Port {
                    port: l.port,
                    addr: l.addr,
                    pid: l.pid,
                    process: l.process,
                    term_id: r.term_id.clone(),
                    line_id: r.line_id.clone(),
                    project: project(&r.cwd),
                    since_ms: now,
                },
            ))
        })
        .collect();
    Some(ports)
}

fn roots() -> Vec<Root> {
    crate::term::all()
        .iter()
        .map(|t| t.info())
        .filter(|i| !i.exited && i.pid > 0)
        .map(|i| Root {
            pid: i.pid,
            term_id: i.id,
            line_id: i.line_id,
            cwd: i.cwd,
        })
        .collect()
}

struct Table {
    ports: BTreeMap<u16, Port>,
    at_ms: i64,
    last: Option<(u64, i64)>,
}

static TABLE: Mutex<Table> = Mutex::new(Table {
    ports: BTreeMap::new(),
    at_ms: i64::MIN,
    last: None,
});

fn update(t: &mut Table) {
    let now = now_ms();
    let Some(mut new) = scan_since(&roots(), now, &mut t.last) else {
        t.at_ms = now;
        return;
    };
    let (opened, closed) = diff(&t.ports, &new);
    for p in new.values_mut() {
        if let Some(o) = t.ports.get(&p.port).filter(|o| !closed.contains(o)) {
            p.since_ms = o.since_ms;
        }
    }
    for p in &closed {
        crate::board::publish("port.closed", p.to_json());
    }
    for p in &opened {
        crate::board::publish("port.opened", p.to_json());
    }
    t.ports = new;
    t.at_ms = now;
}

pub fn refresh() {
    update(&mut TABLE.lock().unwrap_or_else(|e| e.into_inner()));
}

pub fn list() -> Value {
    let mut t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    if now_ms().saturating_sub(t.at_ms) >= EVERY_MS {
        update(&mut t);
    }
    Value::Array(t.ports.values().map(Port::to_json).collect())
}

fn owned_port(ports: &BTreeMap<u16, Port>, port: u16, pid: u32) -> Result<Port, (u16, String)> {
    ports
        .get(&port)
        .filter(|p| p.pid == pid)
        .cloned()
        .ok_or_else(|| {
            (
                404,
                format!("no process {pid} listening on port {port} in an Overseer terminal"),
            )
        })
}

#[cfg(unix)]
fn terminate(pid: u32) -> Result<(), (u16, String)> {
    let pid = i32::try_from(pid).map_err(|_| (400, format!("bad pid {pid}")))?;
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    Err(match e.raw_os_error() {
        Some(libc::ESRCH) => (410, format!("process {pid} has already exited")),
        Some(libc::EPERM) => (403, format!("not allowed to stop process {pid}")),
        _ => (500, format!("stopping process {pid}: {e}")),
    })
}

#[cfg(not(unix))]
fn terminate(_pid: u32) -> Result<(), (u16, String)> {
    Err((
        501,
        "stopping a process is not supported on this platform".into(),
    ))
}

pub fn kill(port: &str, body: &Value) -> Result<Value, (u16, String)> {
    let port: u16 = port
        .parse()
        .map_err(|_| (400, format!("bad port {port:?}")))?;
    let pid = body["pid"]
        .as_u64()
        .and_then(|p| u32::try_from(p).ok())
        .ok_or((400, "pid (number) required".into()))?;
    if body["confirm"] != true {
        return Err((400, "kill needs {\"confirm\":true}".into()));
    }
    let mut t = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    if now_ms().saturating_sub(t.at_ms) >= EVERY_MS {
        update(&mut t);
    }
    let p = owned_port(&t.ports, port, pid)?;
    terminate(pid)?;
    Ok(p.to_json())
}

pub fn watch() {
    loop {
        refresh();
        std::thread::sleep(Duration::from_millis(EVERY_MS as u64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1435 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 111 1 0000000000000000 100 0 0 10 0
   1: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 222 1 0000000000000000 100 0 0 10 0
   2: 0100007F:A1B2 0100007F:1435 01 00000000:00000000 00:00000000 00000000  1000        0 333 1 0000000000000000 20 4 30 10 -1
   3: 0A00020F:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 444 1 0000000000000000 100 0 0 10 0
";

    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:0BB8 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 555 1 0000000000000000 100 0 0 10 0
   1: 00000000000000000000000000000000:1F91 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 666 1 0000000000000000 100 0 0 10 0
   2: 0000000000000000FFFF00000100007F:2328 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 777 1 0000000000000000 100 0 0 10 0
   3: 00000000000000000000000001000000:C350 00000000000000000000000001000000:0BB8 01 00000000:00000000 00:00000000 00000000  1000        0 888 1 0000000000000000 100 0 0 10 0
";

    fn l(pid: u32, addr: &str, port: u16) -> Listen {
        Listen {
            pid,
            process: "node".into(),
            addr: addr.into(),
            port,
        }
    }

    fn p(port: u16, pid: u32, addr: &str) -> Port {
        Port {
            port,
            addr: addr.into(),
            pid,
            process: "node".into(),
            term_id: "T1".into(),
            line_id: None,
            project: "/src".into(),
            since_ms: 0,
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn listeners_are_rechecked_only_when_sockets_change_or_time_passes() {
        let roots = vec![Root {
            pid: 7,
            term_id: "t".into(),
            line_id: None,
            cwd: "/w".into(),
        }];
        let a = fingerprint(&roots, &vec![(7u32, vec![3i32])]);
        let b = fingerprint(&roots, &vec![(7u32, vec![3i32, 9])]);
        assert_ne!(a, b);
        let mut last = None;
        assert!(!unchanged(&mut last, a, 1_000));
        assert!(unchanged(&mut last, a, 1_000 + RECHECK_MS - 1));
        assert!(!unchanged(&mut last, b, 2_000));
        assert!(!unchanged(&mut last, b, 2_000 + RECHECK_MS));
    }

    #[test]
    fn parses_proc_net_tcp() {
        assert_eq!(
            parse_proc_net(TCP),
            vec![
                (111, "127.0.0.1".to_string(), 5173),
                (222, "0.0.0.0".to_string(), 8080),
                (444, "15.2.0.10".to_string(), 22),
            ]
        );
    }

    #[test]
    fn parses_proc_net_tcp6() {
        assert_eq!(
            parse_proc_net(TCP6),
            vec![
                (555, "::1".to_string(), 3000),
                (666, "::".to_string(), 8081),
                (777, "127.0.0.1".to_string(), 9000),
            ]
        );
    }

    #[test]
    fn parses_lsof_fields() {
        let out = "p4242\ncnode\nn*:5173\nn127.0.0.1:5174\np99\ncpython3.12\nn[::1]:8000\nn[fe80::1%lo0]:8001\n";
        assert_eq!(
            parse_lsof(out),
            vec![
                Listen {
                    pid: 4242,
                    process: "node".into(),
                    addr: "0.0.0.0".into(),
                    port: 5173
                },
                Listen {
                    pid: 4242,
                    process: "node".into(),
                    addr: "127.0.0.1".into(),
                    port: 5174
                },
                Listen {
                    pid: 99,
                    process: "python3.12".into(),
                    addr: "::1".into(),
                    port: 8000
                },
                Listen {
                    pid: 99,
                    process: "python3.12".into(),
                    addr: "fe80::1%lo0".into(),
                    port: 8001
                },
            ]
        );
    }

    #[test]
    fn widest_addr_wins_per_port() {
        let got = widest(vec![
            l(1, "127.0.0.1", 80),
            l(1, "::", 80),
            l(1, "::1", 80),
            l(2, "::1", 90),
            l(2, "10.0.0.5", 90),
            l(3, "127.0.0.1", 70),
            l(4, "127.0.0.1", 70),
        ]);
        assert_eq!(
            got,
            vec![l(4, "127.0.0.1", 70), l(1, "::", 80), l(2, "10.0.0.5", 90)]
        );
    }

    #[test]
    fn diff_reports_opened_and_closed() {
        let old: BTreeMap<u16, Port> = [
            (80, p(80, 1, "::")),
            (90, p(90, 2, "127.0.0.1")),
            (70, p(70, 3, "::")),
        ]
        .into();
        let new: BTreeMap<u16, Port> = [
            (80, p(80, 1, "::")),
            (90, p(90, 5, "127.0.0.1")),
            (60, p(60, 6, "::")),
        ]
        .into();
        let (opened, closed) = diff(&old, &new);
        assert_eq!(
            opened.iter().map(|p| (p.port, p.pid)).collect::<Vec<_>>(),
            vec![(60, 6), (90, 5)]
        );
        assert_eq!(
            closed.iter().map(|p| (p.port, p.pid)).collect::<Vec<_>>(),
            vec![(70, 3), (90, 2)]
        );
    }

    #[test]
    fn owners_cover_descendants() {
        let roots = vec![Root {
            pid: 10,
            term_id: "A".into(),
            line_id: None,
            cwd: "/".into(),
        }];
        let own = owners(&roots, &[(10, 1), (11, 10), (12, 11), (13, 1)]);
        assert_eq!(own.get(&12), Some(&0));
        assert_eq!(own.get(&13), None);
    }

    #[test]
    fn only_the_listed_pid_of_a_port_can_be_stopped() {
        let ports = BTreeMap::from([(5173, p(5173, 42, "127.0.0.1"))]);
        assert_eq!(owned_port(&ports, 5173, 42).unwrap().pid, 42);
        assert_eq!(owned_port(&ports, 5173, 43).unwrap_err().0, 404);
        assert_eq!(owned_port(&ports, 8080, 42).unwrap_err().0, 404);
        assert_eq!(
            owned_port(&ports, 5173, 43).unwrap_err().1,
            "no process 43 listening on port 5173 in an Overseer terminal"
        );
    }

    #[test]
    fn kill_validates_its_input_before_touching_any_process() {
        let cases = [
            ("x", json!({"pid": 1, "confirm": true})),
            ("70000", json!({"pid": 1, "confirm": true})),
            ("5173", json!({"confirm": true})),
            ("5173", json!({"pid": "1", "confirm": true})),
            ("5173", json!({"pid": 1})),
        ];
        for (port, body) in cases {
            assert_eq!(kill(port, &body).unwrap_err().0, 400, "{port} {body}");
        }
    }

    #[test]
    fn port_json_shape() {
        let mut port = p(5173, 4242, "127.0.0.1");
        port.line_id = Some("L1".into());
        assert_eq!(
            port.to_json().to_string(),
            r#"{"port":5173,"addr":"127.0.0.1","pid":4242,"process":"node","termId":"T1","lineId":"L1","project":"/src","since":"1970-01-01T00:00:00Z"}"#
        );
        assert!(p(1, 1, "::").to_json().get("lineId").is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detects_own_listener() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let cwd = env!("CARGO_MANIFEST_DIR").to_string();
        let roots = vec![Root {
            pid: std::process::id(),
            term_id: "T".into(),
            line_id: Some("L".into()),
            cwd: cwd.clone(),
        }];
        let got = scan(&roots, 5);
        let p = got.get(&port).expect("own listener detected");
        assert_eq!(p.pid, std::process::id());
        assert_eq!(p.addr, "127.0.0.1");
        assert_eq!(p.term_id, "T");
        assert_eq!(p.line_id.as_deref(), Some("L"));
        assert!(cwd.starts_with(&p.project));
        assert!(!p.process.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn children_walk_matches_the_full_process_table() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30 & wait"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let roots = vec![Root {
            pid: child.id(),
            term_id: "T".into(),
            line_id: None,
            cwd: "/".into(),
        }];
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut own = owned(&roots);
        while own.len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            own = owned(&roots);
        }
        let full = owners(&roots, &ppids());
        for p in own.keys() {
            unsafe { libc::kill(*p as i32, libc::SIGKILL) };
        }
        let _ = child.wait();
        assert_eq!(own.len(), 2, "{own:?}");
        assert_eq!(own, full);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rescans_only_when_owned_sockets_change() {
        let holder = || {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = l.local_addr().unwrap().port();
            let fd: std::os::fd::OwnedFd = l.into();
            let c = std::process::Command::new("sleep")
                .arg("30")
                .stdin(std::process::Stdio::from(fd))
                .spawn()
                .unwrap();
            (c, port)
        };
        let root = |c: &std::process::Child, t: &str| Root {
            pid: c.id(),
            term_id: t.into(),
            line_id: None,
            cwd: "/".into(),
        };
        let (mut a, pa) = holder();
        let (mut b, pb) = holder();
        let mut last = None;
        let one = vec![root(&a, "A")];
        let first = scan_since(&one, 1_000, &mut last).expect("first scan runs");
        assert_eq!(first.get(&pa).map(|p| p.term_id.as_str()), Some("A"));
        assert!(scan_since(&one, 2_000, &mut last).is_none());
        let two = vec![root(&a, "A"), root(&b, "B")];
        let more = scan_since(&two, 3_000, &mut last).expect("new socket rescans");
        assert_eq!(more.get(&pb).map(|p| p.term_id.as_str()), Some("B"));
        assert!(scan_since(&two, 3_500, &mut last).is_none());
        let _ = b.kill();
        let _ = b.wait();
        let fewer = scan_since(&two, 4_000, &mut last).expect("closed socket rescans");
        assert!(fewer.contains_key(&pa) && !fewer.contains_key(&pb));
        assert!(scan_since(&two, 5_000, &mut last).is_none());
        assert!(scan_since(&two, 4_000 + RECHECK_MS, &mut last).is_some());
        assert_eq!(scan_since(&[], 0, &mut last), Some(BTreeMap::new()));
        assert!(last.is_none());
        let _ = a.kill();
        let _ = a.wait();
    }
}
