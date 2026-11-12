use std::process::Command;

pub struct ProcInfo {
    pub start: Option<String>,
    pub argv: Vec<String>,
    pub start_ms: Option<i64>,
}

pub trait Probe {
    fn info(&self, pid: u32) -> Option<ProcInfo>;
    fn list(&self) -> Vec<(u32, Vec<String>)>;
    fn cwd(&self, pid: u32) -> Option<String>;
    fn ppid(&self, _pid: u32) -> Option<u32> {
        None
    }
    fn children(&self, _pid: u32) -> Vec<(u32, Vec<String>)> {
        vec![]
    }
    fn rss(&self, _pid: u32) -> Option<u64> {
        None
    }
}

pub struct System;

fn linux() -> bool {
    cfg!(target_os = "linux")
}

fn cmdline(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
    )
}

/// Parses a macOS `KERN_PROCARGS2` buffer: argc, exec path, NUL padding, then argc NUL-ended args.
pub fn parse_procargs(buf: &[u8]) -> Option<Vec<String>> {
    let raw = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
    let argc = usize::try_from(raw).ok()?;
    let rest = buf.get(4..)?;
    let rest = &rest[rest.iter().position(|b| *b == 0)?..];
    let rest = &rest[rest.iter().position(|b| *b != 0)?..];
    let mut parts = rest.split(|b| *b == 0);
    let argv: Vec<String> = parts
        .by_ref()
        .take(argc)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if argc == 0 || argv.len() < argc || parts.next().is_none() {
        return None;
    }
    Some(argv)
}

#[cfg(target_os = "macos")]
fn procargs(pid: u32) -> Option<Vec<String>> {
    mac::argv(pid)
}

#[cfg(not(target_os = "macos"))]
fn procargs(_pid: u32) -> Option<Vec<String>> {
    None
}

/// ps joins argv with spaces, so its split is only a fallback when the exact argv is unreadable.
fn exact_or_split(pid: u32, split: std::str::SplitWhitespace<'_>) -> Vec<String> {
    procargs(pid).unwrap_or_else(|| split.map(str::to_string).collect())
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    crate::exec::stdout_ok(Command::new(cmd).args(args), crate::exec::PROBE_TIMEOUT)
}

impl Probe for System {
    fn info(&self, pid: u32) -> Option<ProcInfo> {
        if linux() {
            let argv = cmdline(pid)?;
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let after = &stat[stat.rfind(')')? + 1..];
            let start = after.split_whitespace().nth(19).map(str::to_string);
            #[cfg(target_os = "linux")]
            let start_ms = start.as_deref().and_then(boot_ticks_ms);
            #[cfg(not(target_os = "linux"))]
            let start_ms = None;
            Some(ProcInfo {
                start,
                argv,
                start_ms,
            })
        } else {
            #[cfg(target_os = "macos")]
            return mac::argv(pid)
                .filter(|a| !a.is_empty())
                .map(|argv| ProcInfo {
                    start: None,
                    argv,
                    start_ms: mac::start_ms(pid),
                });
            #[allow(unreachable_code)]
            let out = run("ps", &["-o", "etime=,args=", "-p", &pid.to_string()])?;
            let mut it = out.split_whitespace();
            let etime = it.next().and_then(parse_etime);
            let argv = exact_or_split(pid, it);
            (!argv.is_empty()).then_some(ProcInfo {
                start: None,
                argv,
                start_ms: etime.map(|e| now_ms() - e * 1000),
            })
        }
    }

    fn ppid(&self, pid: u32) -> Option<u32> {
        if linux() {
            stat_field(pid, 1)?.parse().ok()
        } else {
            #[cfg(target_os = "macos")]
            return mac::ppid(pid);
            #[allow(unreachable_code)]
            run("ps", &["-o", "ppid=", "-p", &pid.to_string()])?
                .trim()
                .parse()
                .ok()
        }
    }

    fn children(&self, pid: u32) -> Vec<(u32, Vec<String>)> {
        if linux() {
            let Ok(dir) = std::fs::read_dir("/proc") else {
                return vec![];
            };
            dir.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
                .filter(|c| self.ppid(*c) == Some(pid))
                .filter_map(|c| Some((c, cmdline(c).filter(|a| !a.is_empty())?)))
                .collect()
        } else {
            #[cfg(target_os = "macos")]
            return mac::children(pid)
                .into_iter()
                .filter_map(|c| Some((c, mac::argv(c).filter(|a| !a.is_empty())?)))
                .collect();
            #[allow(unreachable_code)]
            run("ps", &["-axo", "pid=,ppid=,args="])
                .unwrap_or_default()
                .lines()
                .filter_map(|l| {
                    let mut it = l.split_whitespace();
                    let c = it.next()?.parse().ok()?;
                    let p: u32 = it.next()?.parse().ok()?;
                    if p != pid {
                        return None;
                    }
                    let argv = exact_or_split(c, it);
                    argv.first()
                        .is_some_and(|a| a != "<defunct>")
                        .then_some((c, argv))
                })
                .collect()
        }
    }

    fn rss(&self, pid: u32) -> Option<u64> {
        let kb: u64 = if linux() {
            std::fs::read_to_string(format!("/proc/{pid}/status"))
                .ok()?
                .lines()
                .find_map(|l| l.strip_prefix("VmRSS:"))?
                .split_whitespace()
                .next()?
                .parse()
                .ok()?
        } else {
            #[cfg(target_os = "macos")]
            return mac::rss(pid);
            #[allow(unreachable_code)]
            run("ps", &["-o", "rss=", "-p", &pid.to_string()])?
                .trim()
                .parse()
                .ok()?
        };
        Some(kb * 1024)
    }

    fn list(&self) -> Vec<(u32, Vec<String>)> {
        if linux() {
            let Ok(dir) = std::fs::read_dir("/proc") else {
                return vec![];
            };
            dir.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
                .filter_map(|pid| Some((pid, cmdline(pid)?)))
                .collect()
        } else {
            #[cfg(target_os = "macos")]
            return mac::pids()
                .into_iter()
                .filter_map(|p| Some((p, mac::argv(p).filter(|a| !a.is_empty())?)))
                .collect();
            #[allow(unreachable_code)]
            run("ps", &["-axo", "pid=,args="])
                .unwrap_or_default()
                .lines()
                .filter_map(|l| {
                    let mut it = l.split_whitespace();
                    let pid = it.next()?.parse().ok()?;
                    Some((pid, exact_or_split(pid, it)))
                })
                .collect()
        }
    }

    fn cwd(&self, pid: u32) -> Option<String> {
        if linux() {
            let p = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
            Some(p.to_string_lossy().into_owned())
        } else {
            #[cfg(target_os = "macos")]
            return mac::cwd(pid);
            #[allow(unreachable_code)]
            run("lsof", &["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])?
                .lines()
                .find_map(|l| l.strip_prefix('n').map(str::to_string))
        }
    }
}

#[cfg(target_os = "macos")]
pub mod mac {
    use std::mem::{size_of, MaybeUninit};

    fn pidinfo<T>(pid: u32, flavor: i32) -> Option<T> {
        let pid = i32::try_from(pid).ok().filter(|p| *p > 0)?;
        let size = size_of::<T>() as i32;
        let mut v = MaybeUninit::<T>::zeroed();
        let n = unsafe { libc::proc_pidinfo(pid, flavor, 0, v.as_mut_ptr().cast(), size) };
        (n == size).then(|| unsafe { v.assume_init() })
    }

    fn bsd(pid: u32) -> Option<libc::proc_bsdinfo> {
        pidinfo(pid, libc::PROC_PIDTBSDINFO)
    }

    pub fn ppid(pid: u32) -> Option<u32> {
        bsd(pid).map(|b| b.pbi_ppid)
    }

    pub fn start_ms(pid: u32) -> Option<i64> {
        bsd(pid).map(|b| (b.pbi_start_tvsec * 1000 + b.pbi_start_tvusec / 1000) as i64)
    }

    pub fn rss(pid: u32) -> Option<u64> {
        pidinfo::<libc::proc_taskinfo>(pid, libc::PROC_PIDTASKINFO).map(|t| t.pti_resident_size)
    }

    pub fn cwd(pid: u32) -> Option<String> {
        let v: libc::proc_vnodepathinfo = pidinfo(pid, libc::PROC_PIDVNODEPATHINFO)?;
        let raw = v.pvi_cdir.vip_path.as_flattened();
        let bytes: Vec<u8> = raw
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn argv(pid: u32) -> Option<Vec<String>> {
        let pid = i32::try_from(pid).ok().filter(|p| *p > 0)?;
        let mut max: libc::c_int = 0;
        let mut len = size_of::<libc::c_int>();
        let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
        let ok = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                2,
                (&mut max as *mut libc::c_int).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if ok != 0 || max <= 0 {
            return None;
        }
        let mut buf = vec![0u8; max as usize];
        let mut size = buf.len();
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let ok = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                buf.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ok != 0 || size < size_of::<libc::c_int>() {
            return None;
        }
        super::parse_procargs(&buf[..size])
    }

    fn pid_list(fill: impl Fn(*mut libc::c_void, i32) -> i32) -> Vec<u32> {
        let mut cap = 256usize;
        loop {
            let mut buf = vec![0i32; cap];
            let bytes = (cap * size_of::<i32>()) as i32;
            let n = fill(buf.as_mut_ptr().cast(), bytes);
            if n <= 0 {
                return vec![];
            }
            let count = n as usize;
            if count < cap || cap >= 1 << 20 {
                buf.truncate(count.min(cap));
                return buf
                    .into_iter()
                    .filter(|p| *p > 0)
                    .map(|p| p as u32)
                    .collect();
            }
            cap *= 4;
        }
    }

    pub fn pids() -> Vec<u32> {
        pid_list(|b, n| unsafe { libc::proc_listallpids(b, n) })
    }

    pub fn children(pid: u32) -> Vec<u32> {
        let Ok(p) = i32::try_from(pid) else {
            return vec![];
        };
        pid_list(|b, n| unsafe { libc::proc_listchildpids(p, b, n) })
    }

    pub fn socket_fds(pid: u32) -> Option<Vec<i32>> {
        let p = i32::try_from(pid).ok().filter(|p| *p > 0)?;
        let need =
            unsafe { libc::proc_pidinfo(p, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        if need <= 0 {
            return None;
        }
        let each = size_of::<libc::proc_fdinfo>();
        let mut buf: Vec<libc::proc_fdinfo> = Vec::with_capacity(need as usize / each + 16);
        let bytes = (buf.capacity() * each) as i32;
        let n = unsafe {
            libc::proc_pidinfo(p, libc::PROC_PIDLISTFDS, 0, buf.as_mut_ptr().cast(), bytes)
        };
        if n <= 0 {
            return None;
        }
        unsafe { buf.set_len(n as usize / each) };
        Some(
            buf.iter()
                .filter(|f| f.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
                .map(|f| f.proc_fd)
                .collect(),
        )
    }
}

fn stat_field(pid: u32, n: usize) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(n).map(str::to_string)
}

#[cfg(unix)]
pub fn alive_not_zombie(pid: i32) -> bool {
    (unsafe { libc::kill(pid, 0) } == 0)
        && !std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .unwrap_or_default()
            .contains(") Z ")
}

pub use crate::util::now_ms;

pub fn parse_etime(s: &str) -> Option<i64> {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<i64>().ok()?, r),
        None => (0, s),
    };
    let parts: Vec<i64> = rest
        .split(':')
        .map(|p| p.parse::<i64>().ok())
        .collect::<Option<_>>()?;
    let (h, m, sec) = match parts[..] {
        [m, s] => (0, m, s),
        [h, m, s] => (h, m, s),
        _ => return None,
    };
    Some(((days * 24 + h) * 60 + m) * 60 + sec)
}

#[cfg(target_os = "linux")]
fn boot_ticks_ms(ticks: &str) -> Option<i64> {
    static BOOT_MS: std::sync::OnceLock<Option<i64>> = std::sync::OnceLock::new();
    let boot = (*BOOT_MS.get_or_init(|| {
        std::fs::read_to_string("/proc/stat")
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("btime "))?
            .trim()
            .parse::<i64>()
            .ok()
            .map(|s| s * 1000)
    }))?;
    let tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    let tck = if tck > 0 { tck as i64 } else { 100 };
    Some(boot + ticks.parse::<i64>().ok()? * 1000 / tck)
}

pub fn same_process(started_at_ms: Option<i64>, proc_start_ms: Option<i64>) -> bool {
    match (started_at_ms, proc_start_ms) {
        (Some(a), Some(b)) => (a - b).abs() <= 60_000,
        _ => true,
    }
}

pub fn mem_bytes() -> Option<(u64, Option<u64>)> {
    if linux() {
        let info = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb = |k: &str| -> Option<u64> {
            info.lines()
                .find_map(|l| l.strip_prefix(k))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
                .map(|v| v * 1024)
        };
        Some((kb("MemTotal:")?, kb("MemAvailable:")))
    } else {
        other_mem()
    }
}

#[cfg(not(target_os = "macos"))]
fn other_mem() -> Option<(u64, Option<u64>)> {
    let total = run("sysctl", &["-n", "hw.memsize"])?.trim().parse().ok()?;
    Some((total, None))
}

#[cfg(target_os = "macos")]
fn other_mem() -> Option<(u64, Option<u64>)> {
    mac_mem()
}

/// Total memory is read once; available (free + inactive + speculative pages) comes from the
/// kernel in-process, so a status tick never spawns `sysctl` or `vm_stat`.
#[cfg(target_os = "macos")]
fn mac_mem() -> Option<(u64, Option<u64>)> {
    extern "C" {
        // Not bound by libc; the kernel page size host_statistics64 counts in (hw.pagesize is
        // per process and reads 4 KiB for an x86_64 build under Rosetta).
        fn host_page_size(host: u32, out: *mut usize) -> i32;
    }
    static INIT: std::sync::OnceLock<Option<(u64, u64, u32)>> = std::sync::OnceLock::new();
    // libc deprecates mach_host_self in favour of mach2, which this crate does not depend on.
    #[allow(deprecated)]
    let (total, page, host) = (*INIT.get_or_init(|| unsafe {
        let (mut total, mut tl) = (0u64, size_of::<u64>());
        if libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut total as *mut u64).cast(),
            &mut tl,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let host = libc::mach_host_self();
        let mut page = 0usize;
        (host_page_size(host, &mut page) == 0 && page > 0).then_some((total, page as u64, host))
    }))?;
    let avail = unsafe {
        let mut s: libc::vm_statistics64 = std::mem::zeroed();
        let mut n = libc::HOST_VM_INFO64_COUNT;
        let r = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&mut s as *mut libc::vm_statistics64).cast(),
            &mut n,
        );
        (r == 0).then(|| {
            (s.free_count as u64 + s.inactive_count as u64 + s.speculative_count as u64) * page
        })
    };
    Some((total, avail))
}

pub fn flags_after(argv: &[String], is_bin: impl Fn(&str) -> bool) -> Option<Vec<String>> {
    let i = argv.iter().position(|a| is_bin(a))?;
    Some(argv[i + 1..].to_vec())
}

// Repairs flags stored while argv was still read with `ps`: a JSON value cut at its spaces.
pub fn rejoin_split_json(flags: &[String]) -> Vec<String> {
    let json_ok = |s: &str| serde_json::from_str::<serde_json::Value>(s).is_ok();
    let broken = |f: &str| {
        let v = f
            .strip_prefix("--")
            .and_then(|s| s.split_once('=').map(|(_, v)| v))
            .unwrap_or(f);
        v.starts_with(['{', '[']) && !json_ok(v)
    };
    let mut out = vec![];
    let mut i = 0;
    while i < flags.len() {
        let mut taken = 1;
        if broken(&flags[i]) {
            let mut joined = flags[i].clone();
            for (n, next) in flags[i + 1..].iter().enumerate() {
                joined = format!("{joined} {next}");
                if !broken(&joined) {
                    taken = n + 2;
                    break;
                }
            }
            if taken > 1 {
                out.push(joined);
                i += taken;
                continue;
            }
        }
        out.push(flags[i].clone());
        i += taken;
    }
    out
}

pub fn launch_session(flags: &[String]) -> Option<String> {
    let mut it = flags.iter();
    while let Some(f) = it.next() {
        let v = match f.as_str() {
            "--resume" | "-r" | "--session-id" => it.next().map(String::as_str),
            s => s
                .strip_prefix("--resume=")
                .or_else(|| s.strip_prefix("--session-id=")),
        };
        if let Some(v) = v.filter(|v| {
            !v.is_empty()
                && !v.starts_with('-')
                && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        }) {
            return Some(v.to_string());
        }
    }
    None
}

pub fn strip_resume(flags: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut it = flags.iter().peekable();
    while let Some(f) = it.next() {
        match f.as_str() {
            "--resume" | "-r" | "--session-id" => {
                if it.peek().is_some_and(|v| !v.starts_with('-')) {
                    it.next();
                }
            }
            "--continue" | "-c" => {}
            s if s.starts_with("--resume=") || s.starts_with("--session-id=") => {}
            _ => out.push(f.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_live_pid_is_alive_and_a_missing_one_is_not() {
        assert!(alive_not_zombie(std::process::id() as i32));
        assert!(!alive_not_zombie(i32::MAX));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_memory_is_read_in_process_with_a_cached_total() {
        let (total, avail) = mac_mem().unwrap();
        let (again, _) = mac_mem().unwrap();
        assert_eq!(total, again);
        assert!(avail.is_some_and(|a| a > 0 && a <= total));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_probes_read_this_process() {
        let me = std::process::id();
        let argv = mac::argv(me).expect("argv");
        assert!(
            argv.first().is_some_and(|a| a.contains("overseer")),
            "{argv:?}"
        );
        let here = std::env::current_dir().unwrap().canonicalize().unwrap();
        let cwd = std::path::PathBuf::from(mac::cwd(me).expect("cwd"))
            .canonicalize()
            .unwrap();
        assert_eq!(cwd, here);
        let parent = unsafe { libc::getppid() } as u32;
        assert_eq!(mac::ppid(me), Some(parent));
        assert!(mac::pids().contains(&me));
        assert!(mac::children(parent).contains(&me));
        assert!(mac::rss(me).is_some_and(|r| r > 0));
        let started = mac::start_ms(me).expect("start");
        assert!(
            started <= now_ms() && now_ms() - started < 3_600_000,
            "{started}"
        );
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let fd = std::os::fd::AsRawFd::as_raw_fd(&l);
        assert!(mac::socket_fds(me).is_some_and(|f| f.contains(&fd)));
        let info = System.info(me).expect("info");
        assert_eq!(info.argv, argv);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_info_reads_the_start_time_of_a_process() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let started = System.info(child.id()).and_then(|i| i.start_ms);
        child.kill().unwrap();
        child.wait().unwrap();
        let started = started.expect("start_ms");
        assert!((now_ms() - started).abs() < 5_000, "{started}");
    }

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn reads_the_launch_session() {
        assert_eq!(
            launch_session(&v(&["--session-id", "ab-12"])).as_deref(),
            Some("ab-12")
        );
        assert_eq!(
            launch_session(&v(&["--model", "opus", "-r", "x1"])).as_deref(),
            Some("x1")
        );
        assert_eq!(launch_session(&v(&["--resume=x2"])).as_deref(), Some("x2"));
        assert_eq!(launch_session(&v(&["--resume", "--model"])), None);
        assert_eq!(launch_session(&v(&["--resume"])), None);
        assert_eq!(launch_session(&v(&["--session-id", "../x"])), None);
        assert_eq!(launch_session(&v(&["-c", "--model", "opus"])), None);
    }

    #[test]
    fn procargs_skip_the_exec_path_and_stop_at_argc() {
        let mut b = 2i32.to_ne_bytes().to_vec();
        b.extend_from_slice(b"/usr/bin/claude\0\0\0claude\0--resume\0HOME=/x\0");
        assert_eq!(parse_procargs(&b).unwrap(), ["claude", "--resume"]);
        assert!(parse_procargs(&[1, 0]).is_none());
    }

    #[test]
    fn strips_resume_flags() {
        let f = v(&[
            "--model",
            "opus",
            "-r",
            "x",
            "--resume=y",
            "-c",
            "--session-id",
            "z",
            "--verbose",
        ]);
        assert_eq!(strip_resume(&f), v(&["--model", "opus", "--verbose"]));
        assert_eq!(strip_resume(&v(&["--resume", "--debug"])), v(&["--debug"]));
    }

    #[test]
    fn etime_and_pid_reuse_guard() {
        assert_eq!(parse_etime("05:03"), Some(303));
        assert_eq!(parse_etime("02:05:03"), Some(7503));
        assert_eq!(parse_etime("1-02:05:03"), Some(93903));
        assert_eq!(parse_etime("x"), None);
        assert!(same_process(Some(1_000_000), Some(1_030_000)));
        assert!(!same_process(Some(1_000_000), Some(1_000_000 + 3_600_000)));
        assert!(same_process(None, Some(5)));
    }

    #[test]
    fn procargs_keeps_args_with_spaces() {
        let args = v(&[
            "claude",
            "--mcp-config={\"command\":\"/Users/x/Library/Application Support/cmux/cua\"}",
            "",
            "--verbose",
        ]);
        let mut buf = 4i32.to_ne_bytes().to_vec();
        buf.extend_from_slice(b"/usr/local/bin/claude\0\0\0\0");
        for a in &args {
            buf.extend_from_slice(a.as_bytes());
            buf.push(0);
        }
        buf.extend_from_slice(b"HOME=/Users/x\0PATH=/bin\0\0");
        assert_eq!(parse_procargs(&buf), Some(args));
        assert_eq!(parse_procargs(&buf[..3]), None);
        assert_eq!(parse_procargs(&buf[..30]), None);
        assert_eq!(parse_procargs(&0i32.to_ne_bytes()), None);
    }

    #[cfg(unix)]
    #[test]
    fn live_argv_keeps_spaces() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 5; :", "Application Support/cmux"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let want = v(&["/bin/sh", "-c", "sleep 5; :", "Application Support/cmux"]);
        // The child may still be between fork and exec on a cold start.
        let (mut info, mut kids, mut all) = (None, vec![], vec![]);
        for _ in 0..50 {
            info = System.info(pid).map(|i| i.argv);
            kids = System.children(std::process::id());
            all = System.list();
            if info.as_ref() == Some(&want) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        child.kill().ok();
        child.wait().ok();
        assert_eq!(info, Some(want.clone()));
        assert!(kids.contains(&(pid, want.clone())), "{kids:?}");
        assert!(all.contains(&(pid, want)));
    }

    #[test]
    fn rejoins_json_split_on_spaces() {
        let f = v(&[
            "--mcp-config={\"c\":\"/Application",
            "Support/x\",\"e\":{\"d\":\"/Application",
            "Support/y\"}}",
            "--permission-mode",
            "auto",
        ]);
        assert_eq!(
            rejoin_split_json(&f),
            v(&[
                "--mcp-config={\"c\":\"/Application Support/x\",\"e\":{\"d\":\"/Application Support/y\"}}",
                "--permission-mode",
                "auto",
            ])
        );
        assert_eq!(rejoin_split_json(&v(&["--x={", "a"])), v(&["--x={", "a"]));
    }

    #[test]
    fn own_stat_parses() {
        if linux() {
            let i = System.info(std::process::id()).unwrap();
            assert!(i.start.unwrap().parse::<u64>().is_ok());
        }
    }
}
