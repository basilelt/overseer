use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::{mpsc, oneshot, watch};

use crate::transport::wire;

pub const OPEN: u8 = wire::MUX_OPEN;
pub const OPEN_OK: u8 = wire::MUX_OPEN_OK;
pub const OPEN_ERR: u8 = wire::MUX_OPEN_ERR;
pub const DATA: u8 = wire::MUX_DATA;
pub const WINDOW: u8 = wire::MUX_WINDOW;
pub const EOF: u8 = wire::MUX_EOF;
pub const CLOSE: u8 = wire::MUX_CLOSE;
pub const PING: u8 = wire::MUX_PING;
pub const PONG: u8 = wire::MUX_PONG;

pub const INITIAL_WINDOW: u32 = 262_144;
pub const MAX_DATA: usize = 32_768;
pub const MAX_PAYLOAD: usize = 65_536;
pub const BANNER: &str = match std::str::from_utf8(wire::MUX_BANNER.split_last().unwrap().1) {
    Ok(s) => s,
    Err(_) => panic!(),
};

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub kind: u8,
    pub id: u32,
    pub payload: Vec<u8>,
}

pub fn encode(kind: u8, id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + payload.len());
    out.push(kind);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Frame> {
    let mut head = [0u8; 9];
    r.read_exact(&mut head).await?;
    let id = u32::from_be_bytes([head[1], head[2], head[3], head[4]]);
    let len = u32::from_be_bytes([head[5], head[6], head[7], head[8]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("mux frame of {len} bytes"),
        ));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).await?;
    Ok(Frame {
        kind: head[0],
        id,
        payload,
    })
}

enum Incoming {
    Data(Vec<u8>),
    Eof,
}

#[derive(Default)]
struct Shared {
    credit: u32,
    waker: Option<Waker>,
    reset: bool,
}

struct Slot {
    shared: Arc<Mutex<Shared>>,
    tx: mpsc::UnboundedSender<Incoming>,
    opened: Option<oneshot::Sender<Result<(), String>>>,
}

pub struct Mux {
    out: mpsc::UnboundedSender<Vec<u8>>,
    streams: Mutex<HashMap<u32, Slot>>,
    next: AtomicU32,
    dead: watch::Sender<Option<String>>,
    heard_ms: AtomicU64,
    born: Instant,
}

impl Mux {
    pub fn start<R, W>(reader: R, writer: W, ping: Duration) -> Arc<Mux>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (out, rx) = mpsc::unbounded_channel();
        let mux = Arc::new(Mux {
            out,
            streams: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
            dead: watch::channel(None).0,
            heard_ms: AtomicU64::new(0),
            born: Instant::now(),
        });
        tokio::spawn(write_loop(mux.clone(), writer, rx));
        tokio::spawn(read_loop(mux.clone(), reader));
        tokio::spawn(ping_loop(Arc::downgrade(&mux), ping));
        mux
    }

    fn send(&self, frame: Vec<u8>) -> bool {
        self.out.send(frame).is_ok()
    }

    pub fn is_dead(&self) -> bool {
        self.dead.borrow().is_some()
    }

    pub fn reason(&self) -> Option<String> {
        self.dead.borrow().clone()
    }

    pub async fn dead(&self) -> String {
        let mut rx = self.dead.subscribe();
        let r = match rx.wait_for(|d| d.is_some()).await {
            Ok(d) => d.clone().unwrap_or_default(),
            Err(_) => "mux dropped".into(),
        };
        r
    }

    pub fn kill(&self, reason: &str) {
        let first = self.dead.send_if_modified(|d| {
            if d.is_some() {
                return false;
            }
            *d = Some(reason.to_string());
            true
        });
        if !first {
            return;
        }
        let slots: Vec<Slot> = self
            .streams
            .lock()
            .unwrap()
            .drain()
            .map(|(_, s)| s)
            .collect();
        for mut s in slots {
            if let Some(o) = s.opened.take() {
                let _ = o.send(Err(reason.to_string()));
            }
            let mut sh = s.shared.lock().unwrap();
            sh.reset = true;
            if let Some(w) = sh.waker.take() {
                w.wake();
            }
        }
    }

    #[cfg(test)]
    pub fn open_streams(&self) -> usize {
        self.streams.lock().unwrap().len()
    }

    pub async fn open(self: &Arc<Self>, target: &str) -> io::Result<MuxStream> {
        if let Some(r) = self.reason() {
            return Err(io::Error::new(io::ErrorKind::NotConnected, r));
        }
        let mut id = self.next.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            id = self.next.fetch_add(1, Ordering::Relaxed);
        }
        let shared = Arc::new(Mutex::new(Shared {
            credit: INITIAL_WINDOW,
            ..Default::default()
        }));
        let (tx, rx) = mpsc::unbounded_channel();
        let (otx, orx) = oneshot::channel();
        self.streams.lock().unwrap().insert(
            id,
            Slot {
                shared: shared.clone(),
                tx,
                opened: Some(otx),
            },
        );
        if !self.send(encode(OPEN, id, target.as_bytes())) {
            self.streams.lock().unwrap().remove(&id);
            return Err(io::Error::new(io::ErrorKind::NotConnected, "tunnel closed"));
        }
        let res = match tokio::time::timeout(Duration::from_secs(15), orx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("tunnel closed".into()),
            Err(_) => Err(format!("opening {target} timed out")),
        };
        match res {
            Ok(()) => Ok(MuxStream {
                id,
                mux: self.clone(),
                shared,
                rx,
                buf: Vec::new(),
                pos: 0,
                eof: false,
                sent_eof: false,
            }),
            Err(e) => {
                self.streams.lock().unwrap().remove(&id);
                Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    format!("{target}: {e}"),
                ))
            }
        }
    }

    fn dispatch(&self, f: Frame) {
        self.heard_ms
            .store(self.born.elapsed().as_millis() as u64, Ordering::Relaxed);
        match f.kind {
            OPEN_OK => {
                if let Some(o) = self
                    .streams
                    .lock()
                    .unwrap()
                    .get_mut(&f.id)
                    .and_then(|s| s.opened.take())
                {
                    let _ = o.send(Ok(()));
                }
            }
            OPEN_ERR => {
                if let Some(mut s) = self.streams.lock().unwrap().remove(&f.id) {
                    if let Some(o) = s.opened.take() {
                        let _ = o.send(Err(String::from_utf8_lossy(&f.payload).into_owned()));
                    }
                }
            }
            DATA => {
                if let Some(s) = self.streams.lock().unwrap().get(&f.id) {
                    let _ = s.tx.send(Incoming::Data(f.payload));
                }
            }
            EOF => {
                if let Some(s) = self.streams.lock().unwrap().get(&f.id) {
                    let _ = s.tx.send(Incoming::Eof);
                }
            }
            WINDOW if f.payload.len() == 4 => {
                let n =
                    u32::from_be_bytes([f.payload[0], f.payload[1], f.payload[2], f.payload[3]]);
                if let Some(s) = self.streams.lock().unwrap().get(&f.id) {
                    let mut sh = s.shared.lock().unwrap();
                    sh.credit = sh.credit.saturating_add(n);
                    if let Some(w) = sh.waker.take() {
                        w.wake();
                    }
                }
            }
            CLOSE => {
                if let Some(s) = self.streams.lock().unwrap().remove(&f.id) {
                    let mut sh = s.shared.lock().unwrap();
                    sh.reset = true;
                    if let Some(w) = sh.waker.take() {
                        w.wake();
                    }
                }
            }
            PING => {
                self.send(encode(PONG, 0, &f.payload));
            }
            _ => {}
        }
    }
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mux: Arc<Mux>,
    mut w: W,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    while let Some(frame) = rx.recv().await {
        let mut res = w.write_all(&frame).await;
        while res.is_ok() {
            match rx.try_recv() {
                Ok(more) => res = w.write_all(&more).await,
                Err(_) => break,
            }
        }
        if let Err(e) = res.and(w.flush().await) {
            mux.kill(&format!("tunnel write failed: {e}"));
            return;
        }
        if mux.is_dead() {
            return;
        }
    }
}

async fn read_loop<R: AsyncRead + Unpin>(mux: Arc<Mux>, mut r: R) {
    loop {
        tokio::select! {
            f = read_frame(&mut r) => match f {
                Ok(f) => mux.dispatch(f),
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return mux.kill("the remote proxy closed the tunnel"),
                Err(e) => return mux.kill(&format!("tunnel read failed: {e}")),
            },
            _ = mux.dead() => return,
        }
    }
}

async fn ping_loop(mux: std::sync::Weak<Mux>, every: Duration) {
    let mut n: u64 = 0;
    loop {
        tokio::time::sleep(every).await;
        let Some(m) = mux.upgrade() else { return };
        if m.is_dead() {
            return;
        }
        let silent = m.born.elapsed().as_millis() as u64 - m.heard_ms.load(Ordering::Relaxed);
        if n >= 2 && silent > 2 * every.as_millis() as u64 + every.as_millis() as u64 / 2 {
            m.kill("no keepalive answer from the remote host");
            return;
        }
        n += 1;
        m.send(encode(PING, 0, &n.to_be_bytes()));
    }
}

pub struct MuxStream {
    id: u32,
    mux: Arc<Mux>,
    shared: Arc<Mutex<Shared>>,
    rx: mpsc::UnboundedReceiver<Incoming>,
    buf: Vec<u8>,
    pos: usize,
    eof: bool,
    sent_eof: bool,
}

impl AsyncRead for MuxStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            if self.pos < self.buf.len() {
                let n = (self.buf.len() - self.pos).min(out.remaining());
                let start = self.pos;
                out.put_slice(&self.buf[start..start + n]);
                self.pos += n;
                if self.pos == self.buf.len() {
                    let done = self.buf.len() as u32;
                    self.buf.clear();
                    self.pos = 0;
                    self.mux.send(encode(WINDOW, self.id, &done.to_be_bytes()));
                }
                return Poll::Ready(Ok(()));
            }
            if self.eof {
                return Poll::Ready(Ok(()));
            }
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(Incoming::Data(d))) => {
                    self.buf = d;
                    self.pos = 0;
                }
                Poll::Ready(Some(Incoming::Eof)) | Poll::Ready(None) => self.eof = true,
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for MuxStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        if data.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let mut sh = self.shared.lock().unwrap();
        if sh.reset || self.mux.is_dead() || self.sent_eof {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "tunnel stream closed",
            )));
        }
        if sh.credit == 0 {
            sh.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let n = data.len().min(MAX_DATA).min(sh.credit as usize);
        sh.credit -= n as u32;
        drop(sh);
        if !self.mux.send(encode(DATA, self.id, &data[..n])) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "tunnel closed",
            )));
        }
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.sent_eof {
            self.sent_eof = true;
            self.mux.send(encode(EOF, self.id, &[]));
        }
        Poll::Ready(Ok(()))
    }
}

impl Drop for MuxStream {
    fn drop(&mut self) {
        if self.mux.streams.lock().unwrap().remove(&self.id).is_some() {
            self.mux.send(encode(CLOSE, self.id, &[]));
        }
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

    pub struct FakeProxy {
        pub frames: mpsc::UnboundedReceiver<Frame>,
        pub w: WriteHalf<DuplexStream>,
    }

    impl FakeProxy {
        pub async fn send(&mut self, kind: u8, id: u32, payload: &[u8]) {
            self.w.write_all(&encode(kind, id, payload)).await.unwrap();
        }

        pub async fn next(&mut self) -> Frame {
            tokio::time::timeout(Duration::from_secs(5), self.frames.recv())
                .await
                .expect("frame")
                .expect("open")
        }

        pub async fn next_kind(&mut self, kind: u8) -> Frame {
            loop {
                let f = self.next().await;
                if f.kind == kind {
                    return f;
                }
            }
        }
    }

    pub fn pair(ping: Duration) -> (Arc<Mux>, FakeProxy) {
        let (a, b) = tokio::io::duplex(1 << 20);
        let (ar, aw) = tokio::io::split(a);
        let (mut br, bw): (ReadHalf<DuplexStream>, WriteHalf<DuplexStream>) = tokio::io::split(b);
        let (tx, frames) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok(f) = read_frame(&mut br).await {
                if tx.send(f).is_err() {
                    return;
                }
            }
        });
        (Mux::start(ar, aw, ping), FakeProxy { frames, w: bw })
    }
}

#[cfg(test)]
mod tests {
    use super::testing::pair;
    use super::*;

    const LONG: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn frames_roundtrip() {
        let bytes = encode(DATA, 7, b"hello");
        assert_eq!(&bytes[..9], &[4, 0, 0, 0, 7, 0, 0, 0, 5]);
        let f = read_frame(&mut &bytes[..]).await.unwrap();
        assert_eq!(
            f,
            Frame {
                kind: DATA,
                id: 7,
                payload: b"hello".to_vec()
            }
        );
        let mut big = encode(DATA, 1, &[]);
        big[5..9].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_be_bytes());
        assert!(read_frame(&mut &big[..]).await.is_err());
    }

    #[tokio::test]
    async fn open_data_eof_and_window() {
        let (mux, mut p) = pair(LONG);
        let m = mux.clone();
        let opening = tokio::spawn(async move { m.open("daemon").await });
        let open = p.next().await;
        assert_eq!(
            (open.kind, open.payload.as_slice()),
            (OPEN, b"daemon".as_slice())
        );
        p.send(OPEN_OK, open.id, &[]).await;
        let mut s = opening.await.unwrap().unwrap();
        s.write_all(b"GET /").await.unwrap();
        s.shutdown().await.unwrap();
        let d = p.next().await;
        assert_eq!(
            (d.kind, d.id, d.payload.as_slice()),
            (DATA, open.id, b"GET /".as_slice())
        );
        assert_eq!(p.next().await.kind, EOF);
        p.send(DATA, open.id, b"HTTP/1.1 200").await;
        p.send(EOF, open.id, &[]).await;
        let mut got = String::new();
        s.read_to_string(&mut got).await.unwrap();
        assert_eq!(got, "HTTP/1.1 200");
        let w = p.next_kind(WINDOW).await;
        assert_eq!(w.payload, 12u32.to_be_bytes());
        assert_eq!(mux.open_streams(), 1);
        drop(s);
        assert_eq!(p.next_kind(CLOSE).await.id, open.id);
        assert_eq!(mux.open_streams(), 0);
    }

    #[tokio::test]
    async fn open_error_is_reported() {
        let (mux, mut p) = pair(LONG);
        let m = mux.clone();
        let opening = tokio::spawn(async move { m.open("tcp:9").await });
        let open = p.next().await;
        p.send(OPEN_ERR, open.id, b"connection refused").await;
        let err = opening.await.unwrap().err().unwrap().to_string();
        assert!(
            err.contains("tcp:9") && err.contains("connection refused"),
            "{err}"
        );
        assert_eq!(mux.open_streams(), 0);
    }

    #[tokio::test]
    async fn writes_stop_at_the_window_and_resume_on_credit() {
        let (mux, mut p) = pair(LONG);
        let m = mux.clone();
        let opening = tokio::spawn(async move { m.open("tcp:80").await });
        let open = p.next().await;
        p.send(OPEN_OK, open.id, &[]).await;
        let mut s = opening.await.unwrap().unwrap();
        let payload = vec![7u8; INITIAL_WINDOW as usize + 1000];
        let writer = tokio::spawn(async move {
            s.write_all(&payload).await.unwrap();
            s
        });
        let mut got = 0usize;
        while got < INITIAL_WINDOW as usize {
            let f = p.next().await;
            assert_eq!(f.kind, DATA);
            assert!(f.payload.len() <= MAX_DATA);
            got += f.payload.len();
        }
        assert_eq!(got, INITIAL_WINDOW as usize);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), p.frames.recv())
                .await
                .is_err()
        );
        assert!(!writer.is_finished());
        p.send(WINDOW, open.id, &4096u32.to_be_bytes()).await;
        let f = p.next().await;
        assert_eq!((f.kind, f.payload.len()), (DATA, 1000));
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn answers_ping_and_dies_without_pong() {
        let (mux, mut p) = pair(Duration::from_millis(50));
        p.send(PING, 0, b"abc").await;
        let pong = p.next_kind(PONG).await;
        assert_eq!(pong.payload, b"abc");
        let reason = tokio::time::timeout(Duration::from_secs(5), mux.dead())
            .await
            .unwrap();
        assert!(reason.contains("keepalive"), "{reason}");
        assert!(mux.open("daemon").await.is_err());
    }

    #[tokio::test]
    async fn stays_alive_while_pongs_come_back() {
        let (mux, mut p) = pair(Duration::from_millis(30));
        for _ in 0..8 {
            let ping = p.next_kind(PING).await;
            p.send(PONG, 0, &ping.payload).await;
        }
        assert!(!mux.is_dead());
    }

    #[tokio::test]
    async fn a_busy_link_without_pongs_stays_alive() {
        let (mux, mut p) = pair(Duration::from_millis(30));
        for _ in 0..20 {
            p.send(DATA, 1, b"output").await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!mux.is_dead(), "{:?}", mux.reason());
        let reason = tokio::time::timeout(Duration::from_secs(5), mux.dead())
            .await
            .unwrap();
        assert!(reason.contains("keepalive"), "{reason}");
    }

    #[tokio::test]
    async fn closing_the_pipe_fails_open_streams() {
        let (mux, mut p) = pair(LONG);
        let m = mux.clone();
        let opening = tokio::spawn(async move { m.open("daemon").await });
        let open = p.next().await;
        p.send(OPEN_OK, open.id, &[]).await;
        let mut s = opening.await.unwrap().unwrap();
        p.w.shutdown().await.unwrap();
        let reason = tokio::time::timeout(Duration::from_secs(5), mux.dead())
            .await
            .unwrap();
        assert!(reason.contains("closed"), "{reason}");
        let mut buf = Vec::new();
        assert_eq!(s.read_to_end(&mut buf).await.unwrap(), 0);
        assert!(s.write_all(b"x").await.is_err());
    }
}
