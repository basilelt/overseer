use crate::sshg::{self, SshCmd};
use serde::Serialize;
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

pub const PRIVACY_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork";
const MDNS: (Ipv4Addr, u16) = (Ipv4Addr::new(224, 0, 0, 251), 5353);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Granted,
    Denied,
    Unreachable,
    Unknown,
    #[default]
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    Reached,
    Blocked,
    Timeout,
    Other,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub status: Status,
    pub host: Option<String>,
    pub endpoint: Option<String>,
    pub via: Option<String>,
    pub addresses: Vec<String>,
    pub private: bool,
    pub multicast: Option<String>,
    pub connect: Option<String>,
    pub interface: Option<String>,
    pub signing: Option<String>,
    pub bundle: Option<String>,
    pub error: Option<String>,
}

pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 10
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 169 && b == 254)
                || (a == 100 && (64..=127).contains(&b))
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_private(IpAddr::V4(v4)),
            None => {
                let s = v6.segments()[0];
                (s & 0xff00) == 0xfd00 || (s & 0xffc0) == 0xfe80
            }
        },
    }
}

pub fn probe_of(r: Result<(), ErrorKind>) -> Probe {
    match r {
        Ok(()) | Err(ErrorKind::ConnectionRefused) => Probe::Reached,
        Err(
            ErrorKind::HostUnreachable
            | ErrorKind::NetworkUnreachable
            | ErrorKind::PermissionDenied,
        ) => Probe::Blocked,
        Err(ErrorKind::TimedOut | ErrorKind::WouldBlock) => Probe::Timeout,
        Err(_) => Probe::Other,
    }
}

pub fn status(multicast: Probe, target: Option<Probe>) -> Status {
    match (multicast, target) {
        (_, Some(Probe::Reached)) => Status::Granted,
        (Probe::Blocked, _) => Status::Denied,
        (Probe::Reached, Some(_)) => Status::Unreachable,
        (Probe::Reached, None) => Status::Granted,
        _ => Status::Unknown,
    }
}

pub fn signing_of(codesign_stderr: &str) -> &'static str {
    let s = codesign_stderr;
    if s.contains("not signed at all") {
        "unsigned"
    } else if s.contains("Signature=adhoc") {
        "adhoc"
    } else if s.contains("Authority=Developer ID Application") {
        "developerId"
    } else if s.contains("Authority=Overseer Self-Signed") {
        "selfSigned"
    } else if s.contains("Authority=") {
        "signed"
    } else {
        "unknown"
    }
}

pub fn route_interface(route_stdout: &str) -> Option<String> {
    route_stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("interface:"))
        .map(|i| i.trim().to_string())
        .filter(|i| !i.is_empty())
}

pub fn mdns_query() -> Vec<u8> {
    let mut q = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in ["_ssh", "_tcp", "local"] {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 12, 0, 1]);
    q
}

fn outcome(r: &io::Result<()>) -> String {
    match r {
        Ok(()) => "ok".into(),
        Err(e) if e.kind() == ErrorKind::ConnectionRefused => format!("reached ({e})"),
        Err(e) => e.to_string(),
    }
}

fn send_multicast() -> io::Result<()> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    sock.send_to(&mdns_query(), MDNS).map(|_| ())
}

fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let app = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf);
    Some(app.unwrap_or(exe))
}

fn codesign(path: &Path) -> Option<&'static str> {
    let out = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=2"])
        .arg(path)
        .output()
        .ok()?;
    Some(signing_of(&String::from_utf8_lossy(&out.stderr)))
}

fn route(ip: IpAddr) -> Option<String> {
    let mut cmd = Command::new("/sbin/route");
    cmd.arg("-n").arg("get");
    if ip.is_ipv6() {
        cmd.arg("-inet6");
    }
    let out = cmd.arg(ip.to_string()).output().ok()?;
    route_interface(&String::from_utf8_lossy(&out.stdout))
}

async fn target_of(host: &str, rep: &mut Report) -> Option<(String, u16)> {
    let (dargs, alias) = sshg::dest_args(host);
    match SshCmd::from_env().resolve_with_hops(&dargs, &alias).await {
        Ok((r, hops)) => {
            if r.proxy_command.is_some() {
                rep.via = Some("ProxyCommand".into());
                None
            } else if r.proxy_jump.is_some() {
                rep.via = r.proxy_jump.clone().map(|j| format!("ProxyJump {j}"));
                hops.first().map(|h| (h.host_name.clone(), h.port))
            } else {
                Some((r.host_name, r.port))
            }
        }
        Err(e) => {
            rep.error = Some(e);
            None
        }
    }
}

pub async fn probe(host: Option<String>) -> Report {
    if !cfg!(target_os = "macos") {
        return Report::default();
    }
    let host = host.map(|h| h.trim().to_string()).filter(|h| !h.is_empty());
    let mut rep = Report {
        host: host.clone(),
        status: Status::Unknown,
        ..Default::default()
    };
    let target = match &host {
        Some(h) => target_of(h, &mut rep).await,
        None => None,
    };
    let mut addrs: Vec<SocketAddr> = Vec::new();
    if let Some((h, p)) = &target {
        rep.endpoint = Some(format!("{h}:{p}"));
        match tokio::net::lookup_host((h.as_str(), *p)).await {
            Ok(found) => {
                for a in found {
                    if !addrs.contains(&a) {
                        addrs.push(a);
                    }
                }
            }
            Err(e) => rep.error = Some(format!("{h}: {e}")),
        }
    }
    rep.addresses = addrs.iter().map(|a| a.ip().to_string()).collect();
    let private = addrs.iter().find(|a| is_private(a.ip())).copied();
    rep.private = private.is_some();
    let bundle = bundle_path();
    rep.bundle = bundle.as_ref().map(|b| b.display().to_string());
    let probed = tokio::task::spawn_blocking(move || {
        let multicast = send_multicast();
        let target = private.map(|a| {
            (
                TcpStream::connect_timeout(&a, CONNECT_TIMEOUT).map(|_| ()),
                route(a.ip()),
            )
        });
        let signing = bundle.as_deref().and_then(codesign);
        (multicast, target, signing)
    })
    .await;
    let Ok((multicast, target, signing)) = probed else {
        return rep;
    };
    rep.multicast = Some(outcome(&multicast));
    rep.signing = signing.map(str::to_string);
    let target_probe = target.map(|(r, iface)| {
        rep.connect = Some(outcome(&r));
        rep.interface = iface;
        probe_of(r.map_err(|e| e.kind()))
    });
    rep.status = status(probe_of(multicast.map_err(|e| e.kind())), target_probe);
    rep
}

#[tauri::command]
pub async fn local_network_status(host: Option<String>) -> Report {
    probe(host).await
}

#[tauri::command]
pub fn open_local_network_settings(app: AppHandle) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("only on macOS".into());
    }
    app.opener()
        .open_url(PRIVACY_URL, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_ranges() {
        for s in [
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.20",
            "169.254.3.4",
            "100.64.0.1",
            "100.127.255.254",
            "fd12::1",
            "fe80::1",
            "::ffff:192.168.0.2",
        ] {
            assert!(is_private(ip(s)), "{s}");
        }
        for s in [
            "8.8.8.8",
            "172.15.0.1",
            "172.32.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "127.0.0.1",
            "::1",
            "2001:db8::1",
            "fc00::1",
            "fec0::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_private(ip(s)), "{s}");
        }
    }

    #[test]
    fn error_kinds_map_to_probes() {
        assert_eq!(probe_of(Ok(())), Probe::Reached);
        assert_eq!(probe_of(Err(ErrorKind::ConnectionRefused)), Probe::Reached);
        assert_eq!(probe_of(Err(ErrorKind::HostUnreachable)), Probe::Blocked);
        assert_eq!(probe_of(Err(ErrorKind::NetworkUnreachable)), Probe::Blocked);
        assert_eq!(probe_of(Err(ErrorKind::PermissionDenied)), Probe::Blocked);
        assert_eq!(probe_of(Err(ErrorKind::TimedOut)), Probe::Timeout);
        assert_eq!(probe_of(Err(ErrorKind::AddrInUse)), Probe::Other);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn errnos_map_to_probes() {
        let kind = |n| io::Error::from_raw_os_error(n).kind();
        assert_eq!(probe_of(Err(kind(113))), Probe::Blocked);
        assert_eq!(probe_of(Err(kind(101))), Probe::Blocked);
        assert_eq!(probe_of(Err(kind(1))), Probe::Blocked);
        assert_eq!(probe_of(Err(kind(111))), Probe::Reached);
        assert_eq!(probe_of(Err(kind(110))), Probe::Timeout);
    }

    #[test]
    fn status_combines_multicast_and_target() {
        use Probe::*;
        assert_eq!(status(Reached, None), Status::Granted);
        assert_eq!(status(Reached, Some(Reached)), Status::Granted);
        assert_eq!(status(Blocked, Some(Reached)), Status::Granted);
        assert_eq!(status(Blocked, None), Status::Denied);
        assert_eq!(status(Blocked, Some(Blocked)), Status::Denied);
        assert_eq!(status(Reached, Some(Blocked)), Status::Unreachable);
        assert_eq!(status(Reached, Some(Timeout)), Status::Unreachable);
        assert_eq!(status(Other, Some(Blocked)), Status::Unknown);
        assert_eq!(status(Timeout, None), Status::Unknown);
    }

    #[test]
    fn codesign_output() {
        assert_eq!(signing_of("Executable=/Applications/Overseer.app/Contents/MacOS/overseer\nSignature=adhoc\nTeamIdentifier=not set"), "adhoc");
        assert_eq!(
            signing_of("/Applications/Overseer.app: code object is not signed at all"),
            "unsigned"
        );
        assert_eq!(signing_of("Authority=Developer ID Application: Example (ABCDE12345)\nTeamIdentifier=ABCDE12345"), "developerId");
        assert_eq!(signing_of("Authority=Apple Development: someone"), "signed");
        assert_eq!(
            signing_of(
                "Signature size=4718\nAuthority=Overseer Self-Signed\nTeamIdentifier=not set"
            ),
            "selfSigned"
        );
        assert_eq!(signing_of(""), "unknown");
    }

    #[test]
    fn route_output() {
        let out = "   route to: 100.92.1.7\ndestination: 100.64.0.0\n       mask: 255.192.0.0\n  interface: utun4\n      flags: <UP,DONE,STATIC>\n";
        assert_eq!(route_interface(out).as_deref(), Some("utun4"));
        assert_eq!(
            route_interface("route: writing to routing socket: not in table"),
            None
        );
    }

    #[test]
    fn mdns_query_is_one_ptr_question() {
        let q = mdns_query();
        assert_eq!(&q[4..6], &[0, 1]);
        assert_eq!(&q[12..17], b"\x04_ssh");
        assert_eq!(&q[q.len() - 5..], &[0, 0, 12, 0, 1]);
    }

    #[test]
    fn report_serializes_camel_case() {
        let v = serde_json::to_value(Report::default()).unwrap();
        assert_eq!(v["status"], "notApplicable");
        assert!(v["addresses"].as_array().unwrap().is_empty());
        assert_eq!(
            serde_json::to_value(Status::Unreachable).unwrap(),
            "unreachable"
        );
    }
}
