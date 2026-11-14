mod common;

use common::Fx;
use overseer::procs::System;
use overseer::util::now_ms;
use overseer::{board, daemon, scan, Ctx};
use serde_json::Value;
use std::io::{Cursor, Read, Write};
use std::time::{Duration, Instant};

struct Mem(Cursor<Vec<u8>>, Vec<u8>);

impl Write for &mut Mem {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.1.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl daemon::Stream for &mut Mem {
    fn set_read_timeout(&self, _: Option<Duration>) -> std::io::Result<()> {
        Ok(())
    }
}

impl Read for &mut Mem {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(b)
    }
}

fn get(target: &str) -> (Duration, String) {
    let mut m = Mem(
        Cursor::new(format!("GET {target} HTTP/1.1\r\nHost: unix\r\n\r\n").into_bytes()),
        vec![],
    );
    let start = Instant::now();
    daemon::conn(&mut m, None);
    let took = start.elapsed();
    let out = String::from_utf8_lossy(&m.1).into_owned();
    assert!(out.starts_with("HTTP/1.1 200"), "{target}: {out:.200}");
    (took, out.split_once("\r\n\r\n").unwrap().1.to_string())
}

#[test]
fn fifty_lines_over_three_stations_stay_responsive() {
    let fx = Fx::new("perf", 20);
    fx.station("vm-b", 15, now_ms(), "idle");
    fx.station("mac-c", 15, now_ms() - 3_600_000, "held");
    for (k, v) in fx.vars() {
        std::env::set_var(k, v);
    }

    let (_, b) = get("/v1/board");
    let b: Value = serde_json::from_str(&b).unwrap();
    assert_eq!(b["lines"].as_array().unwrap().len(), 50);
    assert_eq!(b["stations"].as_array().unwrap().len(), 3);

    for target in ["/board", "/v1/lines", "/v1/board"] {
        let runs: Vec<Duration> = (0..20).map(|_| get(target).0).collect();
        let max = runs.iter().max().unwrap();
        let avg = runs.iter().sum::<Duration>() / runs.len() as u32;
        eprintln!(
            "{target}: avg {avg:?}, max {max:?} over {} runs",
            runs.len()
        );
        assert!(*max < Duration::from_millis(1500), "{target} max {max:?}");
        assert!(avg < Duration::from_millis(500), "{target} avg {avg:?}");
    }

    let probe = System;
    let subs: Vec<_> = (0..50).map(|_| board::subscribe(None).0).collect();
    let ctx = Ctx::from_env(&probe);
    let lines = scan(&ctx).unwrap();
    board::observe(&ctx, &lines);
    fx.station("vm-b", 15, now_ms(), "active");
    let start = Instant::now();
    board::observe(&ctx, &lines);
    let mut worst = Duration::ZERO;
    for rx in &subs {
        let mut n = 0;
        while n < 15 {
            let f = rx.recv_timeout(Duration::from_secs(2)).expect("event");
            if f.contains("event: line.state_changed") {
                n += 1;
            }
        }
        worst = worst.max(start.elapsed());
    }
    eprintln!("sse fanout: 15 state changes to 50 subscribers in {worst:?}");
    assert!(worst < Duration::from_millis(500), "fanout {worst:?}");
    assert!(board::subscribers() >= 50);
}
