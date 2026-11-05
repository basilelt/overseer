use futures_util::{FutureExt, SinkExt, StreamExt};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async, WebSocketStream};

#[derive(Debug, Clone, PartialEq)]
pub enum TermInput {
    Data(String),
    Resize(u16, u16),
    Background(bool),
}

pub const BATCH_MAX: usize = 64 * 1024;
pub const BACKGROUND_DELAY: Duration = Duration::from_millis(200);

pub const CLIENT: &str = if cfg!(any(target_os = "android", target_os = "ios")) {
    "phone"
} else {
    "desktop"
};

pub fn resize_frame(cols: u16, rows: u16) -> Message {
    Message::Text(
        serde_json::json!({"resize": {"cols": cols, "rows": rows}, "client": CLIENT})
            .to_string()
            .into(),
    )
}

pub async fn connect<S>(stream: S, path: &str) -> Result<WebSocketStream<S>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (ws, _) = client_async(format!("ws://localhost{path}"), stream)
        .await
        .map_err(|e| format!("websocket handshake failed: {e}"))?;
    Ok(ws)
}

pub async fn pump<S, F>(
    ws: WebSocketStream<S>,
    mut input: mpsc::Receiver<TermInput>,
    mut on_output: F,
) where
    S: AsyncRead + AsyncWrite + Unpin,
    F: FnMut(Vec<u8>),
{
    let (mut tx, mut rx) = ws.split();
    let mut pending: Vec<u8> = Vec::new();
    let mut background = false;
    let mut due: Option<Instant> = None;
    let mut flush = |pending: &mut Vec<u8>, due: &mut Option<Instant>| {
        *due = None;
        if !pending.is_empty() {
            on_output(std::mem::take(pending));
        }
    };
    loop {
        let wait = async move {
            match due {
                Some(t) => tokio::time::sleep_until(t).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            msg = rx.next() => {
                let mut msg = msg;
                let mut closed = false;
                loop {
                    match msg {
                        Some(Ok(Message::Binary(b))) => pending.extend_from_slice(&b),
                        Some(Ok(Message::Text(t))) => pending.extend_from_slice(t.as_bytes()),
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                            closed = true;
                            break;
                        }
                        Some(Ok(_)) => {}
                    }
                    if pending.len() >= BATCH_MAX {
                        break;
                    }
                    match rx.next().now_or_never() {
                        Some(next) => msg = next,
                        None => break,
                    }
                }
                if closed {
                    flush(&mut pending, &mut due);
                    break;
                }
                if !background || pending.len() >= BATCH_MAX {
                    flush(&mut pending, &mut due);
                } else if due.is_none() && !pending.is_empty() {
                    due = Some(Instant::now() + BACKGROUND_DELAY);
                }
            },
            _ = wait => flush(&mut pending, &mut due),
            inp = input.recv() => {
                let msg = match inp {
                    Some(TermInput::Data(d)) => Message::Binary(d.into_bytes().into()),
                    Some(TermInput::Resize(c, r)) => resize_frame(c, r),
                    Some(TermInput::Background(b)) => {
                        background = b;
                        if !b {
                            flush(&mut pending, &mut due);
                        }
                        continue;
                    }
                    None => {
                        flush(&mut pending, &mut due);
                        let _ = tx.send(Message::Close(None)).await;
                        break;
                    }
                };
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    #[tokio::test]
    #[allow(clippy::result_large_err)]
    async fn terminal_io_over_a_generic_stream() {
        let (client, server) = duplex(64 * 1024);
        let srv = tokio::spawn(async move {
            let mut path = String::new();
            let mut ws = accept_hdr_async(server, |req: &Request, res: Response| {
                path = req.uri().path().to_string();
                Ok(res)
            })
            .await
            .unwrap();
            ws.send(Message::Binary(b"scrollback".to_vec().into()))
                .await
                .unwrap();
            let mut seen = Vec::new();
            while let Some(Ok(m)) = ws.next().await {
                match m {
                    Message::Close(_) => break,
                    m => seen.push(m),
                }
            }
            (path, seen)
        });
        let ws = connect(client, "/v1/terms/t1/io").await.unwrap();
        let (tx, rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let pumping = tokio::spawn(pump(ws, rx, move |b| out_tx.send(b).unwrap()));
        assert_eq!(out_rx.recv().await.unwrap(), b"scrollback");
        tx.send(TermInput::Data("ls\r".into())).await.unwrap();
        tx.send(TermInput::Resize(100, 30)).await.unwrap();
        drop(tx);
        pumping.await.unwrap();
        let (path, seen) = srv.await.unwrap();
        assert_eq!(path, "/v1/terms/t1/io");
        assert_eq!(seen[0], Message::Binary(b"ls\r".to_vec().into()));
        let Message::Text(t) = &seen[1] else {
            panic!("expected text frame")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(t).unwrap(),
            serde_json::json!({"resize": {"cols": 100, "rows": 30}, "client": "desktop"})
        );
    }

    async fn server(
        frames: tokio::sync::mpsc::UnboundedReceiver<Option<&'static [u8]>>,
    ) -> (
        WebSocketStream<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<()>,
    ) {
        let (client, srv) = duplex(1024 * 1024);
        let mut frames = frames;
        let h = tokio::spawn(async move {
            let mut ws = tokio_tungstenite::accept_async(srv).await.unwrap();
            while let Some(f) = frames.recv().await {
                match f {
                    Some(b) => ws.send(Message::Binary(b.to_vec().into())).await.unwrap(),
                    None => ws.flush().await.unwrap(),
                }
            }
            let _ = ws.close(None).await;
        });
        (connect(client, "/v1/terms/t1/io").await.unwrap(), h)
    }

    #[tokio::test]
    async fn frames_already_received_are_delivered_together() {
        let (ftx, frx) = tokio::sync::mpsc::unbounded_channel();
        for f in [&b"a"[..], b"b", b"c"] {
            ftx.send(Some(f)).unwrap();
        }
        let (ws, srv) = server(frx).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let (_tx, rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let pumping = tokio::spawn(pump(ws, rx, move |b| out_tx.send(b).unwrap()));
        assert_eq!(out_rx.recv().await.unwrap(), b"abc");
        drop(ftx);
        pumping.await.unwrap();
        srv.await.unwrap();
    }

    #[tokio::test]
    async fn background_terminals_get_output_in_batches() {
        let (ftx, frx) = tokio::sync::mpsc::unbounded_channel();
        let (ws, srv) = server(frx).await;
        let (tx, rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let pumping = tokio::spawn(pump(ws, rx, move |b| out_tx.send(b).unwrap()));
        tx.send(TermInput::Background(true)).await.unwrap();
        tokio::task::yield_now().await;
        let t0 = Instant::now();
        ftx.send(Some(b"x")).unwrap();
        ftx.send(Some(b"y")).unwrap();
        assert_eq!(out_rx.recv().await.unwrap(), b"xy");
        assert!(t0.elapsed() >= BACKGROUND_DELAY);
        ftx.send(Some(b"z")).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(out_rx.try_recv().is_err());
        tx.send(TermInput::Background(false)).await.unwrap();
        assert_eq!(out_rx.recv().await.unwrap(), b"z");
        let t1 = Instant::now();
        ftx.send(Some(b"now")).unwrap();
        assert_eq!(out_rx.recv().await.unwrap(), b"now");
        assert!(t1.elapsed() < BACKGROUND_DELAY);
        drop(ftx);
        drop(tx);
        pumping.await.unwrap();
        srv.await.unwrap();
    }

    #[tokio::test]
    async fn background_output_is_flushed_when_the_terminal_closes() {
        let (ftx, frx) = tokio::sync::mpsc::unbounded_channel();
        let (ws, srv) = server(frx).await;
        let (tx, rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let pumping = tokio::spawn(pump(ws, rx, move |b| out_tx.send(b).unwrap()));
        tx.send(TermInput::Background(true)).await.unwrap();
        tokio::task::yield_now().await;
        ftx.send(Some(b"bye")).unwrap();
        drop(ftx);
        pumping.await.unwrap();
        assert_eq!(out_rx.recv().await.unwrap(), b"bye");
        srv.await.unwrap();
    }
}
