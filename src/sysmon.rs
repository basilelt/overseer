use crate::http::{q, respond_raw, Request};
use crate::procs::now_ms;
use crate::util::format_rfc3339;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

pub const EVERY_MS: u64 = 2000;
pub const WINDOW: usize = 300;
const LEASE_MS: i64 = 5_000;
pub const FS_EVERY_MS: i64 = 30_000;
const TOP: usize = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub t: i64,
    pub cpu: Option<f64>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub rx: f64,
    pub tx: f64,
    pub read: f64,
    pub write: f64,
}

impl Point {
    fn to_json(self) -> Value {
        json!({"t": self.t, "cpu": self.cpu.map(r1), "memUsed": self.mem_used, "memTotal": self.mem_total,
               "swapUsed": self.swap_used, "rx": self.rx.round(), "tx": self.tx.round(),
               "read": self.read.round(), "write": self.write.round()})
    }
}

pub trait Source: Send {
    fn sample(&mut self, now_ms: i64) -> (Value, Point);
}

type Make = Box<dyn Fn() -> Box<dyn Source> + Send + Sync>;

#[derive(Default)]
struct State {
    subs: Vec<Sender<Arc<String>>>,
    lease_until: i64,
    running: bool,
    ring: VecDeque<Point>,
    current: Option<Arc<Value>>,
    current_t: i64,
    seq: u64,
}

pub struct Hub {
    st: Mutex<State>,
    ready: Condvar,
    every: Duration,
    prime: Duration,
    lease_ms: i64,
    make: Make,
}

impl Hub {
    pub fn new(every: Duration, prime: Duration, lease_ms: i64, make: Make) -> Arc<Self> {
        Arc::new(Hub {
            st: Mutex::new(State::default()),
            ready: Condvar::new(),
            every,
            prime,
            lease_ms,
            make,
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wanted(st: &State, now: i64) -> bool {
        !st.subs.is_empty() || now < st.lease_until
    }

    pub fn running(&self) -> bool {
        self.lock().running
    }

    pub fn subscribers(&self) -> usize {
        self.lock().subs.len()
    }

    fn wake(self: &Arc<Self>, st: &mut State) {
        if st.running {
            return;
        }
        st.running = true;
        let hub = self.clone();
        if std::thread::Builder::new()
            .name("sysmon".into())
            .spawn(move || hub.run())
            .is_err()
        {
            st.running = false;
        }
    }

    pub fn lease(self: &Arc<Self>) {
        let mut st = self.lock();
        st.lease_until = st.lease_until.max(now_ms() + self.lease_ms);
        self.wake(&mut st);
    }

    pub fn subscribe(self: &Arc<Self>) -> (Receiver<Arc<String>>, Option<Arc<String>>) {
        let (tx, rx) = channel();
        let mut st = self.lock();
        st.subs.push(tx);
        let first = st
            .current
            .as_ref()
            .filter(|_| now_ms() - st.current_t < 2 * self.every.as_millis() as i64)
            .map(|v| frame(st.seq, v));
        self.wake(&mut st);
        (rx, first)
    }

    fn run(self: Arc<Self>) {
        let mut src = (self.make)();
        let primed = catch_unwind(AssertUnwindSafe(|| src.sample(now_ms()))).is_ok();
        if primed {
            std::thread::sleep(self.prime);
        }
        loop {
            let now = now_ms();
            let out = if primed {
                catch_unwind(AssertUnwindSafe(|| src.sample(now))).ok()
            } else {
                None
            };
            let mut st = self.lock();
            let Some((full, point)) = out else {
                st.running = false;
                self.ready.notify_all();
                return;
            };
            st.ring.push_back(point);
            while st.ring.len() > WINDOW {
                st.ring.pop_front();
            }
            st.seq += 1;
            let full = Arc::new(full);
            let f = frame(st.seq, &full);
            st.subs.retain(|s| s.send(f.clone()).is_ok());
            st.current = Some(full);
            st.current_t = now;
            self.ready.notify_all();
            if !Self::wanted(&st, now_ms()) {
                st.running = false;
                return;
            }
            drop(st);
            std::thread::sleep(self.every);
        }
    }

    pub fn snapshot(self: &Arc<Self>, since: Option<i64>, wait: Duration) -> Value {
        self.lease();
        let fresh = 2 * self.every.as_millis() as i64;
        let st = self.lock();
        let (st, _) = self
            .ready
            .wait_timeout_while(st, wait, |s| s.running && now_ms() - s.current_t >= fresh)
            .unwrap_or_else(|e| e.into_inner());
        let history: Vec<Value> = st
            .ring
            .iter()
            .filter(|p| since.is_none_or(|s| p.t > s))
            .map(|p| p.to_json())
            .collect();
        json!({
            "intervalMs": self.every.as_millis() as u64,
            "windowMs": self.every.as_millis() as u64 * WINDOW as u64,
            "running": st.running,
            "current": st.current.as_deref().cloned().unwrap_or(Value::Null),
            "history": history,
        })
    }
}

fn frame(seq: u64, data: &Value) -> Arc<String> {
    let env = json!({"id": seq.to_string(), "type": "system.sample", "at": format_rfc3339(now_ms()), "data": data});
    Arc::new(crate::board::sse_frame(seq, "system.sample", &env))
}

fn hub() -> &'static Arc<Hub> {
    static HUB: OnceLock<Arc<Hub>> = OnceLock::new();
    HUB.get_or_init(|| {
        Hub::new(
            Duration::from_millis(EVERY_MS),
            Duration::from_millis(500),
            LEASE_MS,
            Box::new(platform),
        )
    })
}

pub fn active() -> bool {
    hub().running()
}

pub fn serve(r: &Request, s: &mut dyn Write) -> bool {
    let segs: Vec<&str> = r.path.trim_matches('/').split('/').collect();
    match (r.method.as_str(), segs.as_slice()) {
        ("GET", ["v1", "system"]) => {
            let since = q(r, "since").and_then(|v| v.parse().ok());
            let v = hub().snapshot(since, Duration::from_millis(1500));
            respond_raw(s, 200, "application/json", v.to_string().as_bytes());
        }
        ("GET", ["v1", "system", "stream"]) => stream(s),
        _ => return false,
    }
    true
}

fn stream(s: &mut dyn Write) {
    let (rx, first) = hub().subscribe();
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\nretry: 2000\n\n";
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    if let Some(f) = first {
        if s.write_all(f.as_bytes()).is_err() {
            return;
        }
    }
    let _ = s.flush();
    loop {
        let out = match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(f) => s.write_all(f.as_bytes()),
            Err(RecvTimeoutError::Timeout) => s.write_all(crate::board::SSE_PING),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if out.and_then(|_| s.flush()).is_err() {
            return;
        }
    }
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub cpu: f64,
    pub rss: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fs {
    pub mount: String,
    pub device: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mem {
    pub total: u64,
    pub available: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Debug, Default)]
pub struct Snap {
    pub platform: &'static str,
    pub uptime: Option<u64>,
    pub load: Option<[f64; 3]>,
    pub cpu: Option<f64>,
    pub cores: Vec<f64>,
    pub pressure: Option<[Option<f64>; 3]>,
    pub mem: Mem,
    pub net: Vec<(String, f64, f64)>,
    pub disks: Vec<(String, f64, f64)>,
    pub fs: Vec<Fs>,
    pub procs: Vec<Proc>,
}

pub struct Root {
    pub pid: u32,
    pub term_id: String,
    pub line_id: Option<String>,
    pub agent: Option<String>,
    pub title: String,
    pub cwd: String,
}

pub fn term_roots() -> Vec<Root> {
    crate::term::all()
        .iter()
        .map(|t| t.info())
        .filter(|i| !i.exited && i.pid > 0)
        .map(|i| Root {
            pid: i.pid,
            term_id: i.id,
            line_id: i.line_id,
            agent: i.agent,
            title: i.title,
            cwd: i.cwd,
        })
        .collect()
}

pub fn agents(procs: &[Proc], roots: &[Root]) -> Vec<Value> {
    let mut kids: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut at: HashMap<u32, usize> = HashMap::new();
    for (i, p) in procs.iter().enumerate() {
        kids.entry(p.ppid).or_default().push(i);
        at.insert(p.pid, i);
    }
    let mut out: Vec<(f64, Value)> = roots
        .iter()
        .filter_map(|r| {
            let start = *at.get(&r.pid)?;
            let (mut cpu, mut rss, mut n) = (0.0, 0u64, 0usize);
            let mut stack = vec![start];
            while let Some(i) = stack.pop() {
                let p = &procs[i];
                cpu += p.cpu;
                rss += p.rss;
                n += 1;
                if n > 4096 {
                    break;
                }
                if let Some(c) = kids.get(&p.pid) {
                    stack.extend(c.iter().copied().filter(|&c| procs[c].pid != p.pid));
                }
            }
            Some((
                cpu,
                json!({"termId": r.term_id, "lineId": r.line_id, "agent": r.agent, "title": r.title,
                       "cwd": r.cwd, "pid": r.pid, "procs": n, "cpu": r1(cpu), "rss": rss}),
            ))
        })
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.into_iter().map(|(_, v)| v).collect()
}

fn proc_json(p: &Proc) -> Value {
    json!({"pid": p.pid, "name": p.name, "cpu": r1(p.cpu), "rss": p.rss})
}

pub fn finish(s: Snap, now: i64, roots: &[Root]) -> (Value, Point) {
    let (rx, tx) = s.net.iter().fold((0.0, 0.0), |a, n| (a.0 + n.1, a.1 + n.2));
    let (read, write) = s
        .disks
        .iter()
        .fold((0.0, 0.0), |a, d| (a.0 + d.1, a.1 + d.2));
    let used = s.mem.total.saturating_sub(s.mem.available);
    let point = Point {
        t: now,
        cpu: s.cpu,
        mem_used: used,
        mem_total: s.mem.total,
        swap_used: s.mem.swap_used,
        rx,
        tx,
        read,
        write,
    };
    let mut by_cpu: Vec<&Proc> = s.procs.iter().collect();
    by_cpu.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.rss.cmp(&a.rss)));
    let mut by_mem: Vec<&Proc> = s.procs.iter().collect();
    by_mem.sort_by_key(|p| std::cmp::Reverse(p.rss));
    let pressure = s
        .pressure
        .map(|[c, m, i]| json!({"cpu": c, "memory": m, "io": i}));
    let v = json!({
        "t": now,
        "platform": s.platform,
        "supported": true,
        "uptimeSec": s.uptime,
        "load": s.load,
        "cpu": {"total": s.cpu.map(r1), "cores": s.cores.iter().map(|c| r1(*c)).collect::<Vec<_>>()},
        "pressure": pressure,
        "mem": {"total": s.mem.total, "used": used, "available": s.mem.available, "cached": s.mem.cached,
                "swapTotal": s.mem.swap_total, "swapUsed": s.mem.swap_used},
        "net": s.net.iter().map(|(n, r, t)| json!({"name": n, "rx": r.round(), "tx": t.round()})).collect::<Vec<_>>(),
        "disks": s.disks.iter().map(|(n, r, w)| json!({"name": n, "read": r.round(), "write": w.round()})).collect::<Vec<_>>(),
        "fs": s.fs.iter().map(|f| json!({"mount": f.mount, "device": f.device, "fstype": f.fstype,
                                          "total": f.total, "used": f.used, "avail": f.avail})).collect::<Vec<_>>(),
        "top": {"cpu": by_cpu.iter().take(TOP).map(|p| proc_json(p)).collect::<Vec<_>>(),
                "mem": by_mem.iter().take(TOP).map(|p| proc_json(p)).collect::<Vec<_>>()},
        "agents": agents(&s.procs, roots),
    });
    (v, point)
}

pub fn rates(
    prev: &[(String, u64, u64)],
    cur: &[(String, u64, u64)],
    secs: f64,
) -> Vec<(String, f64, f64)> {
    cur.iter()
        .map(|(n, a, b)| {
            let p = prev.iter().find(|x| &x.0 == n);
            let r = |c: u64, o: Option<u64>| {
                o.map_or(0.0, |o| c.saturating_sub(o) as f64 / secs.max(0.001))
            };
            (n.clone(), r(*a, p.map(|p| p.1)), r(*b, p.map(|p| p.2)))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

pub fn parse_stat(text: &str) -> Vec<CpuTimes> {
    text.lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            let v: Vec<u64> = l
                .split_whitespace()
                .skip(1)
                .take(8)
                .filter_map(|x| x.parse().ok())
                .collect();
            let total: u64 = v.iter().sum();
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            CpuTimes {
                busy: total.saturating_sub(idle),
                total,
            }
        })
        .collect()
}

pub fn pct(prev: CpuTimes, cur: CpuTimes) -> f64 {
    let t = cur.total.saturating_sub(prev.total);
    if t == 0 {
        return 0.0;
    }
    (cur.busy.saturating_sub(prev.busy) as f64 * 100.0 / t as f64).clamp(0.0, 100.0)
}

pub fn parse_meminfo(text: &str) -> Mem {
    let mut m: HashMap<&str, u64> = HashMap::new();
    for l in text.lines() {
        if let Some((k, v)) = l.split_once(':') {
            if let Some(n) = v.split_whitespace().next().and_then(|n| n.parse().ok()) {
                m.insert(k.trim(), n);
            }
        }
    }
    let kb = |k: &str| m.get(k).copied().unwrap_or(0) * 1024;
    let total = kb("MemTotal");
    let available = if m.contains_key("MemAvailable") {
        kb("MemAvailable")
    } else {
        kb("MemFree") + kb("Buffers") + kb("Cached")
    };
    Mem {
        total,
        available: available.min(total),
        cached: kb("Cached") + kb("Buffers") + kb("SReclaimable"),
        swap_total: kb("SwapTotal"),
        swap_used: kb("SwapTotal").saturating_sub(kb("SwapFree")),
    }
}

const SKIP_IFACE: &[&str] = &[
    "lo", "docker", "veth", "br-", "virbr", "cni", "flannel", "cali", "vxlan", "fwbr", "fwln",
    "fwpr", "tap", "gif", "stf", "awdl", "llw", "anpi",
];

pub fn real_iface(name: &str) -> bool {
    !SKIP_IFACE.iter().any(|p| name.starts_with(p))
}

pub fn parse_net_dev(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let v: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            (v.len() >= 9).then(|| (name.trim().to_string(), v[0], v[8]))
        })
        .filter(|(n, rx, tx)| real_iface(n) && rx + tx > 0)
        .collect()
}

pub fn physical_disk(name: &str) -> bool {
    const VIRTUAL: &[&str] = &["loop", "ram", "zram", "dm-", "sr", "fd", "nbd", "zd"];
    if VIRTUAL.iter().any(|p| name.starts_with(p)) {
        return false;
    }
    if let Some(rest) = name.strip_prefix("nvme") {
        return !rest.contains('p');
    }
    if let Some(rest) = name.strip_prefix("mmcblk") {
        return !rest.contains('p');
    }
    if ["sd", "vd", "xvd", "hd"]
        .iter()
        .any(|p| name.starts_with(p))
    {
        return !name.ends_with(|c: char| c.is_ascii_digit());
    }
    true
}

pub fn parse_diskstats(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let name = f.get(2)?;
            let rd: u64 = f.get(5)?.parse().ok()?;
            let wr: u64 = f.get(9)?.parse().ok()?;
            (physical_disk(name) && rd + wr > 0).then(|| (name.to_string(), rd * 512, wr * 512))
        })
        .collect()
}

const SKIP_FS: &[&str] = &[
    "tmpfs",
    "devtmpfs",
    "overlay",
    "squashfs",
    "proc",
    "sysfs",
    "cgroup",
    "cgroup2",
    "devpts",
    "mqueue",
    "debugfs",
    "tracefs",
    "securityfs",
    "pstore",
    "bpf",
    "autofs",
    "configfs",
    "fusectl",
    "hugetlbfs",
    "binfmt_misc",
    "ramfs",
    "rpc_pipefs",
    "nsfs",
    "efivarfs",
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "9p",
    "afpfs",
    "devfs",
    "nullfs",
];
const SKIP_MOUNT: &[&str] = &[
    "/proc",
    "/sys",
    "/dev",
    "/run",
    "/snap",
    "/var/lib/docker",
    "/var/snap",
    "/private/var/vm",
];

pub fn real_mount(device: &str, mount: &str, fstype: &str) -> bool {
    if SKIP_FS.contains(&fstype) || fstype.starts_with("fuse.") {
        return false;
    }
    if SKIP_MOUNT
        .iter()
        .any(|p| mount == *p || mount.starts_with(&format!("{p}/")))
    {
        return false;
    }
    if let Some(rest) = mount.strip_prefix("/System/Volumes/") {
        return rest == "Data";
    }
    device.starts_with('/') || matches!(fstype, "zfs" | "btrfs" | "fuseblk")
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mounts(text: &str) -> Vec<(String, String, String)> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (dev, mnt, ty) = (
                unescape(f.next()?),
                unescape(f.next()?),
                f.next()?.to_string(),
            );
            real_mount(&dev, &mnt, &ty).then_some((dev, mnt, ty))
        })
        .filter(|(dev, _, _)| seen.insert(dev.clone()))
        .collect()
}

pub fn parse_loadavg(text: &str) -> Option<[f64; 3]> {
    let v: Vec<f64> = text
        .split(|c: char| c.is_whitespace() || c == '{' || c == '}')
        .filter_map(|x| x.parse().ok())
        .take(3)
        .collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

pub fn parse_pressure(text: &str) -> Option<f64> {
    text.lines()
        .find(|l| l.starts_with("some"))?
        .split_whitespace()
        .find_map(|f| f.strip_prefix("avg10="))?
        .parse()
        .ok()
}

#[derive(Clone, Debug, PartialEq)]
pub struct PidStat {
    pub ppid: u32,
    pub name: String,
    pub ticks: u64,
    pub rss_pages: u64,
}

pub fn parse_pid_stat(text: &str) -> Option<PidStat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text.get(open + 1..close)?.to_string();
    let f: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    let num = |i: usize| f.get(i).and_then(|x| x.parse::<u64>().ok());
    Some(PidStat {
        ppid: num(1)? as u32,
        name,
        ticks: num(11)? + num(12)?,
        rss_pages: num(21)?,
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub struct Linux {
        cpu: Vec<CpuTimes>,
        net: Vec<(String, u64, u64)>,
        disk: Vec<(String, u64, u64)>,
        ticks: HashMap<u32, u64>,
        at: i64,
        fs: Vec<Fs>,
        fs_at: i64,
        tck: f64,
        page: u64,
    }

    fn read(p: &str) -> String {
        std::fs::read_to_string(p).unwrap_or_default()
    }

    fn statvfs(mount: &str) -> Option<(u64, u64, u64)> {
        let c = std::ffi::CString::new(mount).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
            return None;
        }
        let fr = s.f_frsize as u64;
        let total = s.f_blocks as u64 * fr;
        let free = s.f_bfree as u64 * fr;
        Some((total, total.saturating_sub(free), s.f_bavail as u64 * fr))
    }

    fn filesystems() -> Vec<Fs> {
        parse_mounts(&read("/proc/self/mounts"))
            .into_iter()
            .filter_map(|(device, mount, fstype)| {
                let (total, used, avail) = statvfs(&mount)?;
                (total > 0).then_some(Fs {
                    mount,
                    device,
                    fstype,
                    total,
                    used,
                    avail,
                })
            })
            .collect()
    }

    impl Linux {
        pub fn new() -> Self {
            let tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
            let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            Linux {
                cpu: vec![],
                net: vec![],
                disk: vec![],
                ticks: HashMap::new(),
                at: 0,
                fs: vec![],
                fs_at: i64::MIN / 2,
                tck: if tck > 0 { tck as f64 } else { 100.0 },
                page: if page > 0 { page as u64 } else { 4096 },
            }
        }

        fn procs(&mut self, secs: Option<f64>) -> Vec<Proc> {
            let mut ticks = HashMap::with_capacity(self.ticks.len());
            let mut out = Vec::with_capacity(self.ticks.len());
            let Ok(dir) = std::fs::read_dir("/proc") else {
                return out;
            };
            for e in dir.flatten() {
                let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                    continue;
                };
                let Some(st) = parse_pid_stat(&read(&format!("/proc/{pid}/stat"))) else {
                    continue;
                };
                let cpu = match (secs, self.ticks.get(&pid)) {
                    (Some(s), Some(&prev)) => {
                        st.ticks.saturating_sub(prev) as f64 / self.tck / s * 100.0
                    }
                    _ => 0.0,
                };
                ticks.insert(pid, st.ticks);
                out.push(Proc {
                    pid,
                    ppid: st.ppid,
                    name: st.name,
                    cpu,
                    rss: st.rss_pages * self.page,
                });
            }
            self.ticks = ticks;
            out
        }
    }

    impl Source for Linux {
        fn sample(&mut self, now: i64) -> (Value, Point) {
            let secs = (self.at > 0 && now > self.at).then(|| (now - self.at) as f64 / 1000.0);
            let cpu = parse_stat(&read("/proc/stat"));
            let net = parse_net_dev(&read("/proc/net/dev"));
            let disk = parse_diskstats(&read("/proc/diskstats"));
            let first = self.cpu.is_empty();
            let total =
                (!first).then(|| pct(self.cpu[0], cpu.first().copied().unwrap_or_default()));
            let cores = cpu
                .iter()
                .skip(1)
                .enumerate()
                .map(|(i, c)| self.cpu.get(i + 1).map_or(0.0, |p| pct(*p, *c)))
                .collect();
            let s = secs.unwrap_or(1.0);
            let (net_r, disk_r) = if first {
                (vec![], vec![])
            } else {
                (rates(&self.net, &net, s), rates(&self.disk, &disk, s))
            };
            if now - self.fs_at >= FS_EVERY_MS {
                self.fs = filesystems();
                self.fs_at = now;
            }
            let p = |f: &str| parse_pressure(&read(f));
            let pressure = std::path::Path::new("/proc/pressure").exists().then(|| {
                [
                    p("/proc/pressure/cpu"),
                    p("/proc/pressure/memory"),
                    p("/proc/pressure/io"),
                ]
            });
            let procs = self.procs(secs);
            let snap = Snap {
                platform: "linux",
                uptime: read("/proc/uptime")
                    .split_whitespace()
                    .next()
                    .and_then(|u| u.parse::<f64>().ok())
                    .map(|u| u as u64),
                load: parse_loadavg(&read("/proc/loadavg")),
                cpu: total,
                cores,
                pressure,
                mem: parse_meminfo(&read("/proc/meminfo")),
                net: net_r,
                disks: disk_r,
                fs: self.fs.clone(),
                procs,
            };
            self.cpu = cpu;
            self.net = net;
            self.disk = disk;
            self.at = now;
            finish(snap, now, &term_roots())
        }
    }
}

pub mod mac {
    use super::*;

    pub struct Sysctl {
        pub ncpu: usize,
        pub memsize: u64,
        pub load: Option<[f64; 3]>,
        pub boot_sec: Option<u64>,
        pub swap: (u64, u64),
    }

    fn size(s: &str) -> Option<u64> {
        let s = s.trim();
        let (n, mult) = match s.chars().last()? {
            'K' => (&s[..s.len() - 1], 1024.0),
            'M' => (&s[..s.len() - 1], 1024.0 * 1024.0),
            'G' => (&s[..s.len() - 1], 1024.0 * 1024.0 * 1024.0),
            _ => (s, 1.0),
        };
        n.parse::<f64>().ok().map(|v| (v * mult) as u64)
    }

    pub fn parse_sysctl(text: &str) -> Sysctl {
        let l: Vec<&str> = text.lines().collect();
        let field = |line: &str, key: &str| -> Option<String> {
            let i = line.find(key)? + key.len();
            Some(
                line[i..]
                    .trim_start()
                    .split([',', ' ', '}'])
                    .next()?
                    .to_string(),
            )
        };
        let swap = l.get(4).map_or((0, 0), |s| {
            (
                field(s, "total =").and_then(|v| size(&v)).unwrap_or(0),
                field(s, "used =").and_then(|v| size(&v)).unwrap_or(0),
            )
        });
        Sysctl {
            ncpu: l.first().and_then(|v| v.trim().parse().ok()).unwrap_or(1),
            memsize: l.get(1).and_then(|v| v.trim().parse().ok()).unwrap_or(0),
            load: l.get(2).and_then(|v| parse_loadavg(v)),
            boot_sec: l.get(3).and_then(|v| field(v, "sec =")?.parse().ok()),
            swap,
        }
    }

    pub fn parse_vm_stat(text: &str, total: u64, swap: (u64, u64)) -> Mem {
        let page = text
            .lines()
            .next()
            .and_then(|l| l.split("page size of ").nth(1))
            .and_then(|r| r.split_whitespace().next()?.parse().ok())
            .unwrap_or(4096u64);
        let mut m: HashMap<String, u64> = HashMap::new();
        for l in text.lines().skip(1) {
            if let Some((k, v)) = l.split_once(':') {
                if let Ok(n) = v.trim().trim_end_matches('.').parse::<u64>() {
                    m.insert(k.trim().trim_matches('"').to_string(), n * page);
                }
            }
        }
        let g = |k: &str| m.get(k).copied().unwrap_or(0);
        let used = g("Pages active") + g("Pages wired down") + g("Pages occupied by compressor");
        Mem {
            total,
            available: total.saturating_sub(used),
            cached: g("File-backed pages"),
            swap_total: swap.0,
            swap_used: swap.1,
        }
    }

    pub fn parse_ps(text: &str) -> Vec<Proc> {
        text.lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                let pid = f.next()?.parse().ok()?;
                let ppid = f.next()?.parse().ok()?;
                let cpu: f64 = f.next()?.replace(',', ".").parse().ok()?;
                let rss: u64 = f.next()?.parse().ok()?;
                let comm: Vec<&str> = f.collect();
                let comm = comm.join(" ");
                let name = comm.rsplit('/').next().unwrap_or(&comm).to_string();
                Some(Proc {
                    pid,
                    ppid,
                    name,
                    cpu,
                    rss: rss * 1024,
                })
            })
            .collect()
    }

    pub fn parse_netstat(text: &str) -> Vec<(String, u64, u64)> {
        let mut out: Vec<(String, u64, u64)> = vec![];
        for l in text.lines().skip(1) {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 7 || !f.get(2).is_some_and(|n| n.starts_with("<Link")) {
                continue;
            }
            let n = f.len();
            let (Ok(rx), Ok(tx)) = (f[n - 5].parse::<u64>(), f[n - 2].parse::<u64>()) else {
                continue;
            };
            let name = f[0].trim_end_matches('*').to_string();
            if real_iface(&name) && rx + tx > 0 && !out.iter().any(|o| o.0 == name) {
                out.push((name, rx, tx));
            }
        }
        out
    }

    pub fn parse_df(text: &str) -> Vec<Fs> {
        text.lines()
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() < 6 {
                    return None;
                }
                let n = f.len();
                let num = |i: usize| f[i].parse::<u64>().ok().map(|k| k * 1024);
                let idx = (1..n - 4).find(|&i| f[i].parse::<u64>().is_ok())?;
                let mount = f[idx + 4..].join(" ");
                let device = f[..idx].join(" ");
                let total = num(idx)?;
                let (used, avail) = (num(idx + 1)?, num(idx + 2)?);
                (total > 0 && real_mount(&device, &mount, "")).then_some(Fs {
                    mount,
                    device,
                    fstype: String::new(),
                    total,
                    used,
                    avail,
                })
            })
            .collect()
    }

    #[cfg(target_os = "macos")]
    #[derive(Default)]
    pub struct Mac {
        net: Vec<(String, u64, u64)>,
        at: i64,
        fs: Vec<Fs>,
        fs_at: i64,
    }

    #[cfg(target_os = "macos")]
    fn run(cmd: &str, args: &[&str]) -> String {
        crate::exec::output_within(
            std::process::Command::new(cmd).args(args),
            crate::exec::PROBE_TIMEOUT,
        )
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
    }

    #[cfg(target_os = "macos")]
    impl Source for Mac {
        fn sample(&mut self, now: i64) -> (Value, Point) {
            let sc = parse_sysctl(&run(
                "sysctl",
                &[
                    "-n",
                    "hw.ncpu",
                    "hw.memsize",
                    "vm.loadavg",
                    "kern.boottime",
                    "vm.swapusage",
                ],
            ));
            let mem = parse_vm_stat(&run("vm_stat", &[]), sc.memsize, sc.swap);
            let procs = parse_ps(&run("ps", &["-axo", "pid=,ppid=,pcpu=,rss=,comm="]));
            let net = parse_netstat(&run("netstat", &["-ibn"]));
            let first = self.at == 0;
            let secs = ((now - self.at).max(1) as f64) / 1000.0;
            let net_r = if first {
                vec![]
            } else {
                rates(&self.net, &net, secs)
            };
            if now - self.fs_at >= FS_EVERY_MS {
                self.fs = parse_df(&run("df", &["-kP"]));
                self.fs_at = now;
            }
            let total: f64 = procs.iter().map(|p| p.cpu).sum::<f64>() / sc.ncpu.max(1) as f64;
            let snap = Snap {
                platform: "macos",
                uptime: sc.boot_sec.map(|b| ((now / 1000) as u64).saturating_sub(b)),
                load: sc.load,
                cpu: Some(total.clamp(0.0, 100.0)),
                cores: vec![],
                pressure: None,
                mem,
                net: net_r,
                disks: vec![],
                fs: self.fs.clone(),
                procs,
            };
            self.net = net;
            self.at = now;
            finish(snap, now, &term_roots())
        }
    }
}

pub struct Unsupported;

impl Source for Unsupported {
    fn sample(&mut self, now: i64) -> (Value, Point) {
        (
            json!({"t": now, "platform": std::env::consts::OS, "supported": false}),
            Point {
                t: now,
                ..Point::default()
            },
        )
    }
}

#[cfg(target_os = "linux")]
fn platform() -> Box<dyn Source> {
    Box::new(linux::Linux::new())
}

#[cfg(target_os = "macos")]
fn platform() -> Box<dyn Source> {
    Box::new(mac::Mac::default())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform() -> Box<dyn Source> {
    Box::new(Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const STAT: &str = "cpu  100 0 100 700 100 0 0 0 0 0
cpu0 50 0 50 350 50 0 0 0 0 0
cpu1 50 0 50 350 50 0 0 0 0 0
intr 12345
ctxt 999
";
    const STAT2: &str = "cpu  250 0 150 800 100 0 0 0 0 0
cpu0 150 0 50 400 50 0 0 0 0 0
cpu1 100 0 100 400 50 0 0 0 0 0
";

    const MEMINFO: &str = "MemTotal:       16000000 kB
MemFree:         2000000 kB
MemAvailable:    6000000 kB
Buffers:          500000 kB
Cached:          3000000 kB
SwapCached:            0 kB
SReclaimable:     250000 kB
SwapTotal:       4000000 kB
SwapFree:        3000000 kB
";

    const NETDEV: &str = "Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 622271551  341804    0    0    0     0          0         0 622271551  341804    0    0    0     0       0          0
  eth0: 4788028343 6068819    0  404    0     0          0         0 6225170993 4920731    0    0    0     0       0          0
docker0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0
vethab12: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0
 wlan0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
";

    const DISKSTATS: &str = "   7       0 loop0 10 0 80 0 0 0 0 0 0 0 0 0 0 0 0 0 0
 259       0 nvme0n1 1000 0 2000 10 500 0 4000 20 0 30 30 0 0 0 0 0 0
 259       1 nvme0n1p1 900 0 1800 10 400 0 3000 20 0 30 30 0 0 0 0 0 0
   8       0 sda 10 0 100 0 5 0 50 0 0 0 0 0 0 0 0 0 0
   8       1 sda1 10 0 100 0 5 0 50 0 0 0 0 0 0 0 0 0 0
 253       0 dm-0 10 0 100 0 5 0 50 0 0 0 0 0 0 0 0 0 0
   8      16 sdb 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
";

    const MOUNTS: &str = "sysfs /sys sysfs rw 0 0
proc /proc proc rw 0 0
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/nvme0n1p1 /boot/efi vfat rw 0 0
tmpfs /run tmpfs rw 0 0
/dev/loop3 /snap/core/123 squashfs ro 0 0
overlay /var/lib/docker/overlay2/x/merged overlay rw 0 0
/dev/sdb1 /mnt/My\\040Disk ext4 rw 0 0
/dev/nvme0n1p2 /home/bind ext4 rw 0 0
rpool/data/subvol-101-disk-0 / zfs rw 0 0
server:/export /mnt/nfs nfs4 rw 0 0
lxcfs /proc/cpuinfo fuse.lxcfs rw 0 0
";

    #[test]
    fn cpu_percent_from_stat_deltas() {
        let a = parse_stat(STAT);
        let b = parse_stat(STAT2);
        assert_eq!(a.len(), 3);
        assert_eq!(
            a[0],
            CpuTimes {
                busy: 200,
                total: 1000
            }
        );
        assert_eq!(pct(a[0], b[0]), 200.0 * 100.0 / 300.0);
        assert_eq!(pct(a[1], b[1]), 100.0 * 100.0 / 150.0);
        assert_eq!(pct(a[2], b[2]), 100.0 * 100.0 / 150.0);
        assert_eq!(pct(b[0], a[0]), 0.0);
    }

    #[test]
    fn meminfo_used_available_cached_swap() {
        let m = parse_meminfo(MEMINFO);
        assert_eq!(m.total, 16_000_000 * 1024);
        assert_eq!(m.available, 6_000_000 * 1024);
        assert_eq!(m.cached, 3_750_000 * 1024);
        assert_eq!(m.swap_total, 4_000_000 * 1024);
        assert_eq!(m.swap_used, 1_000_000 * 1024);
    }

    #[test]
    fn net_dev_skips_loopback_virtual_and_idle() {
        assert_eq!(
            parse_net_dev(NETDEV),
            vec![("eth0".to_string(), 4788028343, 6225170993)]
        );
    }

    #[test]
    fn diskstats_keeps_physical_disks_in_bytes() {
        assert_eq!(
            parse_diskstats(DISKSTATS),
            vec![
                ("nvme0n1".to_string(), 2000 * 512, 4000 * 512),
                ("sda".to_string(), 100 * 512, 50 * 512)
            ]
        );
        assert!(physical_disk("mmcblk0") && !physical_disk("mmcblk0p1"));
        assert!(physical_disk("xvda") && !physical_disk("xvda1"));
    }

    #[test]
    fn mounts_keep_real_filesystems_once_per_device() {
        let m = parse_mounts(MOUNTS);
        let mounts: Vec<&str> = m.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(mounts, vec!["/", "/boot/efi", "/mnt/My Disk", "/"]);
        assert_eq!(m[3].2, "zfs");
    }

    #[test]
    fn rates_per_second_and_wrap_safe() {
        let prev = vec![("eth0".to_string(), 1000, 5000)];
        let cur = vec![
            ("eth0".to_string(), 3000, 4000),
            ("new0".to_string(), 10, 10),
        ];
        assert_eq!(
            rates(&prev, &cur, 2.0),
            vec![
                ("eth0".to_string(), 1000.0, 0.0),
                ("new0".to_string(), 0.0, 0.0)
            ]
        );
    }

    #[test]
    fn loadavg_pressure_and_pid_stat() {
        assert_eq!(
            parse_loadavg("0.52 0.58 0.59 1/1234 5678\n"),
            Some([0.52, 0.58, 0.59])
        );
        assert_eq!(
            parse_loadavg("{ 1.92 2.10 2.25 }"),
            Some([1.92, 2.10, 2.25])
        );
        assert_eq!(
            parse_pressure("some avg10=1.25 avg60=0.10 avg300=0.35 total=1\nfull avg10=0.00 avg60=0.00 avg300=0.06 total=2\n"),
            Some(1.25)
        );
        let st = "4242 (tmux: server) S 1 4242 4242 0 -1 4194560 1 0 0 0 150 50 0 0 20 0 1 0 100 1000000 2500 18446744073709551615";
        assert_eq!(
            parse_pid_stat(st),
            Some(PidStat {
                ppid: 1,
                name: "tmux: server".into(),
                ticks: 200,
                rss_pages: 2500
            })
        );
        assert_eq!(parse_pid_stat("garbage"), None);
    }

    fn p(pid: u32, ppid: u32, cpu: f64, rss: u64) -> Proc {
        Proc {
            pid,
            ppid,
            name: format!("p{pid}"),
            cpu,
            rss,
        }
    }

    fn root(pid: u32, agent: &str) -> Root {
        Root {
            pid,
            term_id: format!("T{pid}"),
            line_id: Some(format!("L{pid}")),
            agent: Some(agent.into()),
            title: String::new(),
            cwd: "/src/overseer".into(),
        }
    }

    #[test]
    fn agents_sum_their_process_tree() {
        let procs = vec![
            p(1, 0, 1.0, 10),
            p(10, 1, 2.0, 100),
            p(11, 10, 3.0, 200),
            p(12, 11, 0.5, 50),
            p(20, 1, 1.0, 1000),
            p(30, 1, 9.0, 9),
        ];
        let a = agents(
            &procs,
            &[root(10, "claude"), root(20, "codex"), root(99, "gone")],
        );
        assert_eq!(a.len(), 2);
        assert_eq!(a[0]["agent"], "claude");
        assert_eq!(a[0]["cpu"], 5.5);
        assert_eq!(a[0]["rss"], 350);
        assert_eq!(a[0]["procs"], 3);
        assert_eq!(a[0]["lineId"], "L10");
        assert_eq!(a[1]["agent"], "codex");
        assert_eq!(a[1]["rss"], 1000);
    }

    #[test]
    fn finish_builds_point_and_top_lists() {
        let procs: Vec<Proc> = (1..=15)
            .map(|i| p(i, 0, i as f64, (20 - i) as u64))
            .collect();
        let snap = Snap {
            platform: "linux",
            cpu: Some(12.345),
            mem: Mem {
                total: 100,
                available: 40,
                ..Mem::default()
            },
            net: vec![("eth0".into(), 10.0, 20.0), ("wlan0".into(), 1.0, 2.0)],
            disks: vec![("sda".into(), 5.0, 6.0)],
            procs,
            ..Snap::default()
        };
        let (v, pt) = finish(snap, 1234, &[]);
        assert_eq!(pt.cpu, Some(12.345));
        assert_eq!((pt.mem_used, pt.mem_total), (60, 100));
        assert_eq!((pt.rx, pt.tx, pt.read, pt.write), (11.0, 22.0, 5.0, 6.0));
        assert_eq!(v["cpu"]["total"], 12.3);
        assert_eq!(v["mem"]["used"], 60);
        assert_eq!(v["top"]["cpu"].as_array().unwrap().len(), TOP);
        assert_eq!(v["top"]["cpu"][0]["pid"], 15);
        assert_eq!(v["top"]["mem"][0]["pid"], 1);
        assert_eq!(pt.to_json()["memUsed"], 60);
    }

    const SYSCTL: &str = "10
17179869184
{ 1.92 2.10 2.25 }
{ sec = 1729000000, usec = 123456 } Tue Oct 15 12:00:00 2024
total = 2048.00M  used = 1024.00M  free = 1024.00M  (encrypted)
";
    const VM_STAT: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                               10000.
Pages active:                            100000.
Pages inactive:                           90000.
Pages speculative:                         5000.
Pages wired down:                         50000.
\"Pages occupied by compressor\":           25000.
File-backed pages:                        80000.
";
    const PS: &str = "    1     0   0.5  12000 /sbin/launchd
  501     1  45,0 800000 /Applications/Claude Helper.app/Contents/MacOS/Claude Helper
  bad line
";
    const NETSTAT: &str = "Name       Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll
lo0        16384 <Link#1>                        100     0      10000      100     0      10000     0
en0        1500  <Link#6>    aa:bb:cc:dd:ee:ff  5000     0    7000000     4000     0    3000000     0
en0        1500  192.168.1     192.168.1.10      5000     -    7000000     4000     -    3000000     -
utun0      1380  <Link#20>                          0     0          0        0     0          0     0
";
    const DF: &str = "Filesystem     1024-blocks      Used Available Capacity  Mounted on
/dev/disk3s1s1   971350180  10000000 500000000     2%    /
devfs                  200       200         0   100%    /dev
/dev/disk3s5     971350180 400000000 500000000    45%    /System/Volumes/Data
/dev/disk3s6     971350180   2000000 500000000     1%    /System/Volumes/VM
/dev/disk5s1        100000     50000     50000    50%    /Volumes/My Stick
map auto_home            0         0         0   100%    /System/Volumes/Data/home
";

    #[test]
    fn macos_parsers() {
        let s = mac::parse_sysctl(SYSCTL);
        assert_eq!(s.ncpu, 10);
        assert_eq!(s.memsize, 17179869184);
        assert_eq!(s.load, Some([1.92, 2.10, 2.25]));
        assert_eq!(s.boot_sec, Some(1729000000));
        assert_eq!(s.swap, (2048 * 1024 * 1024, 1024 * 1024 * 1024));
        let m = mac::parse_vm_stat(VM_STAT, 17179869184, s.swap);
        assert_eq!(m.available, 17179869184 - 175000 * 16384);
        assert_eq!(m.cached, 80000 * 16384);
        let ps = mac::parse_ps(PS);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[1].name, "Claude Helper");
        assert_eq!((ps[1].ppid, ps[1].cpu, ps[1].rss), (1, 45.0, 800000 * 1024));
        assert_eq!(
            mac::parse_netstat(NETSTAT),
            vec![("en0".to_string(), 7000000, 3000000)]
        );
        let df: Vec<String> = mac::parse_df(DF).into_iter().map(|f| f.mount).collect();
        assert_eq!(df, vec!["/", "/System/Volumes/Data", "/Volumes/My Stick"]);
    }

    struct Fake(Arc<AtomicUsize>, bool);

    impl Source for Fake {
        fn sample(&mut self, now: i64) -> (Value, Point) {
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            if self.1 && n > 0 {
                panic!("boom");
            }
            (
                json!({"t": now, "n": n}),
                Point {
                    t: now,
                    cpu: Some(n as f64),
                    ..Point::default()
                },
            )
        }
    }

    fn fake_hub(every_ms: u64, lease_ms: i64, panics: bool) -> (Arc<Hub>, Arc<AtomicUsize>) {
        let n = Arc::new(AtomicUsize::new(0));
        let c = n.clone();
        let hub = Hub::new(
            Duration::from_millis(every_ms),
            Duration::from_millis(1),
            lease_ms,
            Box::new(move || Box::new(Fake(c.clone(), panics)) as Box<dyn Source>),
        );
        (hub, n)
    }

    fn until(mut f: impl FnMut() -> bool) -> bool {
        for _ in 0..400 {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn samples_only_while_subscribed() {
        let (hub, n) = fake_hub(10, 0, false);
        std::thread::sleep(Duration::from_millis(40));
        assert!(!hub.running());
        assert_eq!(n.load(Ordering::SeqCst), 0);
        let (rx, _) = hub.subscribe();
        assert!(hub.running());
        let f = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(f.starts_with("id: 1\nevent: system.sample\ndata: {"));
        assert!(f.contains("\"type\":\"system.sample\""));
        drop(rx);
        assert!(until(|| !hub.running()));
        assert_eq!(hub.subscribers(), 0);
        let after = n.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(n.load(Ordering::SeqCst), after);
    }

    #[test]
    fn snapshot_leases_and_returns_history_since() {
        let (hub, _) = fake_hub(10, 80, false);
        let v = hub.snapshot(None, Duration::from_secs(2));
        assert!(hub.running());
        assert_eq!(v["intervalMs"], 10);
        assert_eq!(v["windowMs"], 10 * WINDOW as u64);
        assert!(v["current"]["n"].as_u64().is_some());
        let last = v["history"].as_array().unwrap().last().unwrap()["t"]
            .as_i64()
            .unwrap();
        assert!(until(|| !hub.running()));
        let v = hub.snapshot(Some(last), Duration::from_secs(2));
        assert!(v["history"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["t"].as_i64().unwrap() > last));
        assert!(until(|| !hub.running()));
    }

    #[test]
    fn ring_keeps_ten_minutes() {
        let (hub, n) = fake_hub(0, 0, false);
        let (rx, _) = hub.subscribe();
        assert!(until(|| n.load(Ordering::SeqCst) > WINDOW + 20));
        drop(rx);
        assert!(until(|| !hub.running()));
        let v = hub.snapshot(None, Duration::ZERO);
        let h = v["history"].as_array().unwrap();
        assert_eq!(h.len(), WINDOW);
        assert!(h[0]["cpu"].as_f64().unwrap() > 20.0);
        assert!(until(|| !hub.running()));
    }

    #[test]
    fn panicking_source_stops_and_restarts() {
        let (hub, n) = fake_hub(5, 0, true);
        let (rx, _) = hub.subscribe();
        assert!(until(|| !hub.running()));
        let first = n.load(Ordering::SeqCst);
        let (rx2, _) = hub.subscribe();
        assert!(until(|| n.load(Ordering::SeqCst) > first));
        assert!(until(|| !hub.running()));
        drop((rx, rx2));
    }

    #[test]
    fn live_sample_on_this_machine() {
        let mut s = platform();
        s.sample(now_ms());
        std::thread::sleep(Duration::from_millis(50));
        let (v, pt) = s.sample(now_ms());
        if cfg!(target_os = "linux") {
            assert_eq!(v["supported"], true);
            assert!(pt.mem_total > 0);
            assert!(pt.cpu.is_some());
            assert!(!v["top"]["mem"].as_array().unwrap().is_empty());
        }
    }
}
