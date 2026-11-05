use crate::sshg::Resolved;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr};
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

pub const ARG: &str = "--ssh-relay";
pub const ENV_TOKEN: &str = "OVERSEER_RELAY_TOKEN";
pub const ENV_MODE: &str = "OVERSEER_SSH_RELAY";
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Relay {
    pub addr: SocketAddr,
    pub token: String,
    pub peer: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn wanted(macos: bool, r: &Resolved, mode: Option<&str>) -> bool {
    if r.proxy_command.is_some() || r.proxy_jump.is_some() {
        return false;
    }
    match mode.map(str::trim) {
        Some("1") => true,
        Some("0") => false,
        _ => macos,
    }
}

pub fn family_allows(family: &str, addr: &SocketAddr) -> bool {
    match family.to_ascii_lowercase().as_str() {
        "inet" => addr.is_ipv4(),
        "inet6" => addr.is_ipv6(),
        _ => true,
    }
}

pub fn proxy_command(exe: &Path, addr: SocketAddr) -> String {
    let exe = exe.to_string_lossy().replace('\'', "'\\''");
    format!("'{exe}' {ARG} {addr}").replace('%', "%%")
}

pub async fn connect_tcp(
    host: &str,
    port: u16,
    family: &str,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await?
        .filter(|a| family_allows(family, a))
        .collect();
    let mut last = io::Error::new(
        io::ErrorKind::NotFound,
        format!("{host}: no address for AddressFamily {family}"),
    );
    for a in addrs {
        match tokio::time::timeout(timeout, TcpStream::connect(a)).await {
            Ok(Ok(s)) => {
                let _ = s.set_nodelay(true);
                return Ok(s);
            }
            Ok(Err(e)) => {
                last = io::Error::new(
                    e.kind(),
                    format!("connect to host {host} ({}) port {port}: {e}", a.ip()),
                )
            }
            Err(_) => {
                last = io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "connect to host {host} ({}) port {port}: Operation timed out",
                        a.ip()
                    ),
                )
            }
        }
    }
    Err(last)
}

pub async fn open(remote: TcpStream) -> io::Result<Relay> {
    open_with(remote, ACCEPT_TIMEOUT).await
}

pub async fn open_with(remote: TcpStream, accept_timeout: Duration) -> io::Result<Relay> {
    let peer = remote.peer_addr()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let token = crate::askpass::random_token();
    let want = format!("{token}\n").into_bytes();
    let task = tokio::spawn(async move {
        let mut remote = remote;
        let accepted = tokio::time::timeout(accept_timeout, async {
            loop {
                let Ok((mut local, _)) = listener.accept().await else {
                    return None;
                };
                let mut got = vec![0u8; want.len()];
                let read =
                    tokio::time::timeout(Duration::from_secs(5), local.read_exact(&mut got)).await;
                if matches!(read, Ok(Ok(_))) && got == want {
                    return Some(local);
                }
            }
        })
        .await;
        if let Ok(Some(mut local)) = accepted {
            let _ = local.set_nodelay(true);
            let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
        }
    });
    Ok(Relay {
        addr,
        token,
        peer,
        task,
    })
}

pub fn pipe<R: Read + Send + 'static, W: Write>(
    addr: &str,
    token: &str,
    input: R,
    mut output: W,
) -> io::Result<()> {
    let mut sock = std::net::TcpStream::connect(addr)?;
    let _ = sock.set_nodelay(true);
    sock.write_all(format!("{token}\n").as_bytes())?;
    let mut up = sock.try_clone()?;
    std::thread::spawn(move || {
        let mut input = input;
        let _ = io::copy(&mut input, &mut up);
        let _ = up.shutdown(Shutdown::Write);
    });
    let mut buf = [0u8; 32 * 1024];
    loop {
        let n = sock.read(&mut buf)?;
        if n == 0 {
            return output.flush();
        }
        output.write_all(&buf[..n])?;
        output.flush()?;
    }
}

pub fn client_main() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(ARG) {
        return None;
    }
    let Some(addr) = args.next() else {
        eprintln!("overseer: {ARG} needs an address");
        return Some(2);
    };
    let token = std::env::var(ENV_TOKEN).unwrap_or_default();
    match pipe(&addr, &token, io::stdin(), io::stdout()) {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("overseer relay {addr}: {e}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sshg::parse;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn relays_only_direct_connections() {
        let direct = parse("ws", "hostname 192.168.1.5\nport 22\n");
        assert!(wanted(true, &direct, None));
        assert!(!wanted(false, &direct, None));
        assert!(wanted(false, &direct, Some("1")));
        assert!(!wanted(true, &direct, Some("0")));
        assert!(!wanted(
            true,
            &parse("ws", "hostname w\nproxycommand nc %h %p\n"),
            Some("1")
        ));
        assert!(!wanted(
            true,
            &parse("ws", "hostname w\nproxyjump bastion\n"),
            None
        ));
        assert!(wanted(
            true,
            &parse("ws", "hostname w\nproxycommand none\nproxyjump none\n"),
            None
        ));
    }

    #[test]
    fn address_family_filter() {
        let v4: SocketAddr = "192.168.1.5:22".parse().unwrap();
        let v6: SocketAddr = "[fd00::5]:22".parse().unwrap();
        assert!(family_allows("any", &v4) && family_allows("any", &v6));
        assert!(family_allows("inet", &v4) && !family_allows("inet", &v6));
        assert!(!family_allows("INET6", &v4) && family_allows("inet6", &v6));
        assert_eq!(parse("x", "addressfamily inet\n").address_family, "inet");
        assert_eq!(parse("x", "").address_family, "any");
    }

    #[test]
    fn proxy_command_quotes_the_executable() {
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        assert_eq!(
            proxy_command(
                Path::new("/Applications/Overseer.app/Contents/MacOS/overseer"),
                addr
            ),
            "'/Applications/Overseer.app/Contents/MacOS/overseer' --ssh-relay 127.0.0.1:5000"
        );
        assert_eq!(
            proxy_command(Path::new("/Users/o'neil/100% apps/sb"), addr),
            "'/Users/o'\\''neil/100%% apps/sb' --ssh-relay 127.0.0.1:5000"
        );
    }

    async fn echo_server() -> SocketAddr {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = s.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn relays_bytes_both_ways_through_an_in_process_connection() {
        let echo = echo_server().await;
        let remote = connect_tcp("127.0.0.1", echo.port(), "any", Duration::from_secs(5))
            .await
            .unwrap();
        let relay = open(remote).await.unwrap();
        assert_eq!(relay.peer, echo);
        let (addr, token) = (relay.addr.to_string(), relay.token.clone());
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let sent = payload.clone();
        let out = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            pipe(&addr, &token, io::Cursor::new(sent), &mut out).map(|_| out)
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(out, payload);
    }

    #[tokio::test]
    async fn ignores_clients_without_the_token() {
        let echo = echo_server().await;
        let remote = connect_tcp("127.0.0.1", echo.port(), "any", Duration::from_secs(5))
            .await
            .unwrap();
        let relay = open(remote).await.unwrap();
        let mut bad = TcpStream::connect(relay.addr).await.unwrap();
        bad.write_all(format!("{}\n", "0".repeat(relay.token.len())).as_bytes())
            .await
            .unwrap();
        bad.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        let got = tokio::time::timeout(Duration::from_millis(500), bad.read(&mut buf)).await;
        assert!(!matches!(got, Ok(Ok(n)) if n > 0));
        let (addr, token) = (relay.addr.to_string(), relay.token.clone());
        let out = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            pipe(&addr, &token, io::Cursor::new(b"ping".to_vec()), &mut out).map(|_| out)
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(out, b"ping");
    }

    #[tokio::test]
    async fn the_connection_outlives_the_accept_window() {
        let echo = echo_server().await;
        let remote = connect_tcp("127.0.0.1", echo.port(), "any", Duration::from_secs(5))
            .await
            .unwrap();
        let relay = open_with(remote, Duration::from_millis(300)).await.unwrap();
        let mut local = TcpStream::connect(relay.addr).await.unwrap();
        local
            .write_all(format!("{}\n", relay.token).as_bytes())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(900)).await;
        local.write_all(b"still here").await.unwrap();
        let mut buf = [0u8; 10];
        tokio::time::timeout(Duration::from_secs(5), local.read_exact(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf, b"still here");
    }

    #[tokio::test]
    async fn connect_errors_keep_their_kind_and_name_the_address() {
        let l = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let e = connect_tcp("127.0.0.1", port, "any", Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused);
        assert!(
            e.to_string().starts_with(&format!(
                "connect to host 127.0.0.1 (127.0.0.1) port {port}: "
            )),
            "{e}"
        );
        let e = connect_tcp("127.0.0.1", port, "inet6", Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }
}
