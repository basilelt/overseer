use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::Sha256;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

pub struct TwinFactory {
    pub addr: String,
    pub token: String,
    pub events: Arc<Mutex<Vec<Value>>>,
    pub deliveries: Arc<Mutex<Vec<(bool, Value)>>>,
    pub secret: Arc<Mutex<String>>,
}

fn read_head(r: &mut BufReader<TcpStream>) -> Vec<(String, String)> {
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
            return headers;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
}

pub fn verify(secret: &str, header: &str, body: &str, now_s: i64) -> bool {
    let mut t = None;
    let mut v1 = None;
    for part in header.split(',') {
        match part.split_once('=') {
            Some(("t", x)) => t = x.parse::<i64>().ok(),
            Some(("v1", x)) => v1 = Some(x.to_string()),
            _ => {}
        }
    }
    let (Some(t), Some(v1)) = (t, v1) else {
        return false;
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{t}.{body}").as_bytes());
    let want: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    want == v1 && (now_s - t).abs() < 300
}

impl TwinFactory {
    pub fn new(addr: &str, token: &str) -> TwinFactory {
        TwinFactory {
            addr: addr.into(),
            token: token.into(),
            events: Arc::default(),
            deliveries: Arc::default(),
            secret: Arc::default(),
        }
    }

    pub fn get(&self, path: &str) -> (u16, Value) {
        let mut s = TcpStream::connect(&self.addr).unwrap();
        write!(
            s,
            "GET {path} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            self.token
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = out.split_once("\r\n\r\n").unwrap().1;
        (code, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    pub fn subscribe(&self) {
        let mut s = TcpStream::connect(&self.addr).unwrap();
        write!(
            s,
            "GET /v1/events HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {}\r\nAccept: text/event-stream\r\n\r\n",
            self.token
        )
        .unwrap();
        let events = self.events.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(s);
            read_head(&mut r);
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 0 {
                let l = line.trim_end();
                let data = l
                    .strip_prefix("data:")
                    .or_else(|| l.split_once("data:").map(|x| x.1));
                if let Some(v) = data.and_then(|d| serde_json::from_str::<Value>(d.trim()).ok()) {
                    events.lock().unwrap().push(v);
                }
                line.clear();
            }
        });
    }

    pub fn webhook_receiver(&self) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/twinfactory/hook", l.local_addr().unwrap());
        let (deliveries, secret) = (self.deliveries.clone(), self.secret.clone());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut w = s.try_clone().unwrap();
                let mut r = BufReader::new(s);
                let h = read_head(&mut r);
                let get = |k: &str| {
                    h.iter()
                        .find(|(a, _)| a == k)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                };
                let mut body = vec![0; get("content-length").parse().unwrap_or(0)];
                r.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                let now = overseer::procs::now_ms() / 1000;
                let ok = verify(
                    &secret.lock().unwrap(),
                    &get("x-overseer-signature"),
                    &body,
                    now,
                );
                let ev = serde_json::from_str(&body).unwrap_or(Value::Null);
                deliveries.lock().unwrap().push((ok, ev));
                let _ = w.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
            }
        });
        url
    }
}
