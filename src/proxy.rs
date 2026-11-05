use std::collections::HashMap;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

pub const OPEN: u8 = 1;
pub const OPEN_OK: u8 = 2;
pub const OPEN_ERR: u8 = 3;
pub const DATA: u8 = 4;
pub const WINDOW: u8 = 5;
pub const EOF: u8 = 6;
pub const CLOSE: u8 = 7;
pub const PING: u8 = 8;
pub const PONG: u8 = 9;

pub const BANNER: &[u8] = b"SB:MUX 1\n";
pub const MAX_PAYLOAD: usize = 65536;
pub const MAX_DATA: usize = 32768;
pub const INITIAL_CREDIT: u64 = 262144;

pub trait Duplex: Read + Write + Send {
    fn try_clone_box(&self) -> io::Result<Box<dyn Duplex>>;
    fn shutdown(&self, how: Shutdown) -> io::Result<()>;
}

impl Duplex for TcpStream {
    fn try_clone_box(&self) -> io::Result<Box<dyn Duplex>> {
        Ok(Box::new(self.try_clone()?))
    }
    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        TcpStream::shutdown(self, how)
    }
}

#[cfg(unix)]
impl Duplex for std::os::unix::net::UnixStream {
    fn try_clone_box(&self) -> io::Result<Box<dyn Duplex>> {
        Ok(Box::new(self.try_clone()?))
    }
    fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        std::os::unix::net::UnixStream::shutdown(self, how)
    }
}

pub fn encode(kind: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(9 + payload.len());
    f.push(kind);
    f.extend_from_slice(&stream.to_be_bytes());
    f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    f.extend_from_slice(payload);
    f
}

pub fn decode(r: &mut impl Read) -> io::Result<Option<(u8, u32, Vec<u8>)>> {
    let mut h = [0u8; 9];
    match r.read_exact(&mut h) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let stream = u32::from_be_bytes([h[1], h[2], h[3], h[4]]);
    let len = u32::from_be_bytes([h[5], h[6], h[7], h[8]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame payload of {len} bytes exceeds {MAX_PAYLOAD}"),
        ));
    }
    let mut payload = vec![0u8; len];
    match r.read_exact(&mut payload) {
        Ok(()) => Ok(Some((h[0], stream, payload))),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn open_target(target: &str) -> io::Result<Box<dyn Duplex>> {
    if target == "daemon" {
        return open_daemon();
    }
    let Some(port) = target
        .strip_prefix("tcp:")
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p != 0)
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown target {target}"),
        ));
    };
    let mut last = None;
    for addr in [
        SocketAddr::from(([127, 0, 0, 1], port)),
        SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)),
    ] {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                return Ok(Box::new(s));
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("connect failed")))
}

#[cfg(unix)]
fn open_daemon() -> io::Result<Box<dyn Duplex>> {
    let path = crate::daemon::socket_path(&crate::default_state_dir());
    let s = std::os::unix::net::UnixStream::connect(&path)
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
    Ok(Box::new(s))
}

#[cfg(not(unix))]
fn open_daemon() -> io::Result<Box<dyn Duplex>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "unsupported"))
}

enum Cmd {
    Data(Vec<u8>),
    Eof,
    Close,
}

#[derive(Default)]
struct Flow {
    credit: u64,
    closed: bool,
    sent_eof: bool,
    got_eof: bool,
}

struct Chan {
    id: u32,
    tx: Sender<Cmd>,
    flow: Mutex<Flow>,
    cv: Condvar,
    sock: Mutex<Option<Box<dyn Duplex>>>,
}

type Streams = Arc<Mutex<HashMap<u32, Arc<Chan>>>>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
struct Out(Sender<Vec<u8>>);

impl Out {
    fn send(&self, kind: u8, stream: u32, payload: &[u8]) {
        let _ = self.0.send(encode(kind, stream, payload));
    }
}

fn detach(streams: &Streams, c: &Arc<Chan>) {
    let mut m = lock(streams);
    if m.get(&c.id).is_some_and(|x| Arc::ptr_eq(x, c)) {
        m.remove(&c.id);
    }
}

fn shut(c: &Chan) {
    lock(&c.flow).closed = true;
    c.cv.notify_all();
    if let Some(s) = lock(&c.sock).take() {
        let _ = s.shutdown(Shutdown::Both);
    }
    let _ = c.tx.send(Cmd::Close);
}

fn reset(streams: &Streams, c: &Arc<Chan>, out: &Out) {
    if lock(&c.flow).closed {
        return;
    }
    detach(streams, c);
    shut(c);
    out.send(CLOSE, c.id, &[]);
}

fn finish_if_done(streams: &Streams, c: &Arc<Chan>) {
    let done = {
        let f = lock(&c.flow);
        f.sent_eof && f.got_eof && !f.closed
    };
    if done {
        detach(streams, c);
        shut(c);
    }
}

fn pump_out(streams: Streams, c: Arc<Chan>, mut sock: Box<dyn Duplex>, out: Out) {
    let mut buf = vec![0u8; MAX_DATA];
    loop {
        let want = {
            let mut f = lock(&c.flow);
            while f.credit == 0 && !f.closed {
                f = c.cv.wait(f).unwrap_or_else(|e| e.into_inner());
            }
            if f.closed {
                return;
            }
            (f.credit as usize).min(MAX_DATA)
        };
        match sock.read(&mut buf[..want]) {
            Ok(0) => {
                let mut f = lock(&c.flow);
                if f.closed {
                    return;
                }
                f.sent_eof = true;
                drop(f);
                out.send(EOF, c.id, &[]);
                finish_if_done(&streams, &c);
                return;
            }
            Ok(n) => {
                let mut f = lock(&c.flow);
                if f.closed {
                    return;
                }
                f.credit -= n as u64;
                drop(f);
                out.send(DATA, c.id, &buf[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                reset(&streams, &c, &out);
                return;
            }
        }
    }
}

fn pump_in(streams: Streams, c: Arc<Chan>, mut sock: Box<dyn Duplex>, rx: Receiver<Cmd>, out: Out) {
    for cmd in rx {
        match cmd {
            Cmd::Data(b) => {
                if sock.write_all(&b).and_then(|_| sock.flush()).is_err() {
                    reset(&streams, &c, &out);
                    return;
                }
                if lock(&c.flow).closed {
                    return;
                }
                out.send(WINDOW, c.id, &(b.len() as u32).to_be_bytes());
            }
            Cmd::Eof => {
                let _ = sock.shutdown(Shutdown::Write);
                lock(&c.flow).got_eof = true;
                finish_if_done(&streams, &c);
            }
            Cmd::Close => return,
        }
    }
}

type Opener = Arc<dyn Fn(&str) -> io::Result<Box<dyn Duplex>> + Send + Sync>;

fn connect(
    streams: Streams,
    c: Arc<Chan>,
    rx: Receiver<Cmd>,
    target: String,
    open: Opener,
    out: Out,
) {
    let opened = open(&target).and_then(|s| {
        let r = s.try_clone_box()?;
        let w = s.try_clone_box()?;
        Ok((s, r, w))
    });
    let (ctl, r, w) = match opened {
        Ok(x) => x,
        Err(e) => {
            let was_closed = lock(&c.flow).closed;
            detach(&streams, &c);
            lock(&c.flow).closed = true;
            if !was_closed {
                out.send(OPEN_ERR, c.id, e.to_string().as_bytes());
            }
            return;
        }
    };
    {
        let f = lock(&c.flow);
        if f.closed {
            let _ = ctl.shutdown(Shutdown::Both);
            return;
        }
        *lock(&c.sock) = Some(ctl);
        out.send(OPEN_OK, c.id, &[]);
    }
    let (s2, c2, o2) = (streams.clone(), c.clone(), out.clone());
    std::thread::spawn(move || pump_in(s2, c2, w, rx, o2));
    pump_out(streams, c, r, out);
}

pub fn serve<O>(
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
    open: O,
) -> io::Result<()>
where
    O: Fn(&str) -> io::Result<Box<dyn Duplex>> + Send + Sync + 'static,
{
    let open: Opener = Arc::new(open);
    let (otx, orx) = channel::<Vec<u8>>();
    let (done_tx, done_rx) = channel::<()>();
    std::thread::spawn(move || {
        let mut w = BufWriter::new(output);
        for f in orx {
            if w.write_all(&f).and_then(|_| w.flush()).is_err() {
                break;
            }
        }
        let _ = done_tx.send(());
    });
    let out = Out(otx);
    let _ = out.0.send(BANNER.to_vec());
    let streams: Streams = Arc::default();
    let mut input = BufReader::new(input);
    let result = loop {
        let (kind, id, payload) = match decode(&mut input) {
            Ok(Some(f)) => f,
            Ok(None) => break Ok(()),
            Err(e) => break Err(e),
        };
        let get = || lock(&streams).get(&id).cloned();
        match kind {
            OPEN if id != 0 => {
                if lock(&streams).contains_key(&id) {
                    continue;
                }
                let target = String::from_utf8_lossy(&payload).into_owned();
                let (tx, rx) = channel();
                let c = Arc::new(Chan {
                    id,
                    tx,
                    flow: Mutex::new(Flow {
                        credit: INITIAL_CREDIT,
                        ..Default::default()
                    }),
                    cv: Condvar::new(),
                    sock: Mutex::new(None),
                });
                lock(&streams).insert(id, c.clone());
                let (s, o, op) = (streams.clone(), out.clone(), open.clone());
                std::thread::spawn(move || connect(s, c, rx, target, op, o));
            }
            DATA if !payload.is_empty() => {
                if let Some(c) = get() {
                    let _ = c.tx.send(Cmd::Data(payload));
                }
            }
            WINDOW if payload.len() == 4 => {
                if let Some(c) = get() {
                    let n = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    lock(&c.flow).credit += n as u64;
                    c.cv.notify_all();
                }
            }
            EOF => {
                if let Some(c) = get() {
                    let _ = c.tx.send(Cmd::Eof);
                }
            }
            CLOSE => {
                if let Some(c) = get() {
                    detach(&streams, &c);
                    shut(&c);
                }
            }
            PING => out.send(PONG, 0, &payload),
            _ => {}
        }
    };
    let all: Vec<Arc<Chan>> = lock(&streams).drain().map(|(_, c)| c).collect();
    for c in all {
        shut(&c);
    }
    drop(out);
    let _ = done_rx.recv_timeout(Duration::from_secs(1));
    result
}

pub fn run() -> std::process::ExitCode {
    match serve(io::stdin(), io::stdout(), open_target) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("overseer proxy: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Instant;

    struct Sink(Sender<Vec<u8>>);

    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            let _ = self.0.send(b.to_vec());
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct Frames {
        rx: Receiver<Vec<u8>>,
        buf: Vec<u8>,
    }

    impl Frames {
        fn fill(&mut self, deadline: Instant) -> bool {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(b) => {
                    self.buf.extend_from_slice(&b);
                    true
                }
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => false,
            }
        }

        fn banner(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.buf.len() < BANNER.len() {
                assert!(self.fill(deadline), "no banner");
            }
            assert_eq!(&self.buf[..BANNER.len()], BANNER);
            self.buf.drain(..BANNER.len());
        }

        fn next_within(&mut self, d: Duration) -> Option<(u8, u32, Vec<u8>)> {
            let deadline = Instant::now() + d;
            loop {
                if self.buf.len() >= 9 {
                    let len =
                        u32::from_be_bytes([self.buf[5], self.buf[6], self.buf[7], self.buf[8]])
                            as usize;
                    if self.buf.len() >= 9 + len {
                        let f: Vec<u8> = self.buf.drain(..9 + len).collect();
                        return decode(&mut f.as_slice()).unwrap();
                    }
                }
                if !self.fill(deadline) {
                    return None;
                }
            }
        }

        fn next(&mut self) -> (u8, u32, Vec<u8>) {
            self.next_within(Duration::from_secs(5)).expect("frame")
        }
    }

    fn start<O>(open: O) -> (UnixStream, Frames, std::thread::JoinHandle<io::Result<()>>)
    where
        O: Fn(&str) -> io::Result<Box<dyn Duplex>> + Send + Sync + 'static,
    {
        let (app, proxy_in) = UnixStream::pair().unwrap();
        let (tx, rx) = channel();
        let h = std::thread::spawn(move || serve(proxy_in, Sink(tx), open));
        let mut f = Frames { rx, buf: vec![] };
        f.banner();
        (app, f, h)
    }

    fn send(app: &mut UnixStream, kind: u8, id: u32, p: &[u8]) {
        app.write_all(&encode(kind, id, p)).unwrap();
    }

    fn echo_opener() -> impl Fn(&str) -> io::Result<Box<dyn Duplex>> + Send + Sync + 'static {
        |t: &str| {
            if t != "echo" {
                return open_target(t);
            }
            let (a, mut b) = UnixStream::pair()?;
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = b.read(&mut buf) {
                    if n == 0 || b.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
            Ok(Box::new(a))
        }
    }

    #[test]
    fn frame_roundtrip() {
        let f = encode(DATA, 0xdead_beef, b"hello");
        assert_eq!(&f[..9], &[4, 0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 5]);
        assert_eq!(
            decode(&mut f.as_slice()).unwrap(),
            Some((DATA, 0xdead_beef, b"hello".to_vec()))
        );
        assert_eq!(decode(&mut &[][..]).unwrap(), None);
        let big = [&[DATA, 0, 0, 0, 1][..], &(65537u32).to_be_bytes()].concat();
        assert!(decode(&mut big.as_slice()).is_err());
    }

    #[test]
    fn open_echo_and_half_close() {
        let (mut app, mut f, h) = start(echo_opener());
        send(&mut app, OPEN, 1, b"echo");
        assert_eq!(f.next(), (OPEN_OK, 1, vec![]));
        send(&mut app, DATA, 1, b"ping me");
        let mut got = vec![];
        let mut window = 0u32;
        while got.len() < 7 || window < 7 {
            match f.next() {
                (DATA, 1, p) => got.extend(p),
                (WINDOW, 1, p) => window += u32::from_be_bytes(p.try_into().unwrap()),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(got, b"ping me");
        assert_eq!(window, 7);
        send(&mut app, EOF, 1, &[]);
        assert_eq!(f.next(), (EOF, 1, vec![]));
        drop(app);
        h.join().unwrap().unwrap();
    }

    #[test]
    fn open_errors() {
        let (mut app, mut f, h) = start(echo_opener());
        send(&mut app, OPEN, 3, b"nope");
        let (k, id, msg) = f.next();
        assert_eq!((k, id), (OPEN_ERR, 3));
        assert!(String::from_utf8_lossy(&msg).contains("unknown target"));
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        assert!(open_target(&format!("tcp:{port}")).is_err());
        send(&mut app, OPEN, 3, format!("tcp:{port}").as_bytes());
        assert_eq!(f.next().0, OPEN_ERR);
        drop(app);
        h.join().unwrap().unwrap();
    }

    #[test]
    fn tcp_target_connects() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.write_all(b"hi").unwrap();
        });
        let (mut app, mut f, h) = start(open_target);
        send(&mut app, OPEN, 9, format!("tcp:{port}").as_bytes());
        assert_eq!(f.next(), (OPEN_OK, 9, vec![]));
        assert_eq!(f.next(), (DATA, 9, b"hi".to_vec()));
        assert_eq!(f.next(), (EOF, 9, vec![]));
        drop(app);
        h.join().unwrap().unwrap();
    }

    #[test]
    fn flow_control_stalls_one_stream_only() {
        let opener = {
            let echo = echo_opener();
            move |t: &str| -> io::Result<Box<dyn Duplex>> {
                if t != "flood" {
                    return echo(t);
                }
                let (a, mut b) = UnixStream::pair()?;
                std::thread::spawn(move || {
                    let chunk = vec![7u8; 8192];
                    for _ in 0..50 {
                        if b.write_all(&chunk).is_err() {
                            return;
                        }
                    }
                });
                Ok(Box::new(a))
            }
        };
        let (mut app, mut f, h) = start(opener);
        send(&mut app, OPEN, 1, b"flood");
        assert_eq!(f.next(), (OPEN_OK, 1, vec![]));
        let mut total = 0usize;
        while total < INITIAL_CREDIT as usize {
            let (k, id, p) = f.next();
            assert_eq!((k, id), (DATA, 1));
            assert!(p.len() <= MAX_DATA);
            total += p.len();
        }
        assert_eq!(total, INITIAL_CREDIT as usize);
        assert!(f.next_within(Duration::from_millis(300)).is_none());
        send(&mut app, OPEN, 2, b"echo");
        assert_eq!(f.next(), (OPEN_OK, 2, vec![]));
        send(&mut app, DATA, 2, b"x");
        let mut seen = false;
        while !seen {
            match f.next() {
                (DATA, 2, p) => seen = p == b"x",
                (WINDOW, 2, _) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        send(&mut app, WINDOW, 1, &1000u32.to_be_bytes());
        let mut more = 0;
        while more < 1000 {
            match f.next() {
                (DATA, 1, p) => more += p.len(),
                (WINDOW, 2, _) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(more, 1000);
        send(&mut app, CLOSE, 1, &[]);
        drop(app);
        h.join().unwrap().unwrap();
    }

    #[test]
    fn ping_pong_and_unknown_types() {
        let (mut app, mut f, h) = start(echo_opener());
        send(&mut app, 42, 0, b"ignored");
        send(&mut app, PING, 0, b"abc");
        assert_eq!(f.next(), (PONG, 0, b"abc".to_vec()));
        drop(app);
        h.join().unwrap().unwrap();
    }

    #[test]
    fn oversized_frame_is_fatal() {
        let (mut app, _f, h) = start(echo_opener());
        app.write_all(&[DATA, 0, 0, 0, 1, 0, 1, 0, 1]).unwrap();
        assert!(h.join().unwrap().is_err());
    }
}
