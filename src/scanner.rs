use crate::Line;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const COLD_WAIT: Duration = Duration::from_secs(3);

static SHARED: AtomicBool = AtomicBool::new(false);
static POKED: AtomicBool = AtomicBool::new(false);
static S: Scanner = Scanner::new();

thread_local! {
    static SCANNING: Cell<bool> = const { Cell::new(false) };
}

struct Gate {
    running: bool,
    started: u64,
    done: u64,
    err: Option<String>,
}

pub struct Scanner {
    gate: Mutex<Gate>,
    done: Condvar,
    last: Mutex<Vec<Line>>,
    cold_served: AtomicBool,
}

struct Turn<'a> {
    s: &'a Scanner,
    gen: u64,
    err: Option<String>,
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        let mut g = self.s.gate();
        g.running = false;
        g.done = self.gen;
        g.err = self.err.take();
        self.s.done.notify_all();
    }
}

type Scan<'a> = Box<dyn FnOnce() -> Result<Vec<Line>, String> + 'a>;

impl Scanner {
    pub const fn new() -> Scanner {
        Scanner {
            gate: Mutex::new(Gate {
                running: false,
                started: 0,
                done: 0,
                err: None,
            }),
            done: Condvar::new(),
            last: Mutex::new(vec![]),
            cold_served: AtomicBool::new(false),
        }
    }

    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn last(&self) -> Vec<Line> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn fresh(&self, run: Scan) -> Result<Vec<Line>, String> {
        let mut g = self.gate();
        let want = g.started + 1;
        loop {
            if g.done >= want {
                return match &g.err {
                    Some(e) => Err(e.clone()),
                    None => Ok(self.last()),
                };
            }
            if !g.running {
                g.running = true;
                g.started += 1;
                let mut turn = Turn {
                    s: self,
                    gen: g.started,
                    err: Some("line scan panicked".into()),
                };
                drop(g);
                let r = run();
                match &r {
                    Ok(lines) => {
                        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = lines.clone();
                        turn.err = None;
                    }
                    Err(e) => turn.err = Some(e.clone()),
                }
                drop(turn);
                return r;
            }
            g = self.done.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn current(&self, wait: Duration, run: Scan) -> Result<Vec<Line>, String> {
        let end = Instant::now() + wait;
        let mut g = self.gate();
        loop {
            if g.done > 0 {
                let lines = self.last();
                return match &g.err {
                    Some(e) if lines.is_empty() => Err(e.clone()),
                    _ => Ok(lines),
                };
            }
            if !g.running {
                drop(g);
                return self.fresh(run);
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.cold_served.store(true, Ordering::Release);
                SCANNING.with(|s| s.set(true));
                return Ok(vec![]);
            }
            g = self
                .done
                .wait_timeout(g, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Scanner::new()
    }
}

pub fn share() {
    SHARED.store(true, Ordering::Release);
}

pub fn shared() -> bool {
    SHARED.load(Ordering::Acquire)
}

pub fn fresh(run: impl FnOnce() -> Result<Vec<Line>, String>) -> Result<Vec<Line>, String> {
    if shared() {
        S.fresh(Box::new(run))
    } else {
        run()
    }
}

pub fn current(run: impl FnOnce() -> Result<Vec<Line>, String>) -> Result<Vec<Line>, String> {
    if shared() {
        S.current(COLD_WAIT, Box::new(run))
    } else {
        run()
    }
}

static POKE_LOCK: Mutex<()> = Mutex::new(());
static POKE: Condvar = Condvar::new();

pub fn poke() {
    POKED.store(true, Ordering::Release);
    let _g = POKE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    POKE.notify_all();
}

pub fn take_poke() -> bool {
    POKED.swap(false, Ordering::AcqRel)
}

pub fn wait_poke(limit: Duration) -> bool {
    let end = Instant::now() + limit;
    let mut g = POKE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if take_poke() {
            return true;
        }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        g = POKE
            .wait_timeout(g, left)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

pub fn cached() -> Vec<Line> {
    S.last()
}

pub fn take_scanning() -> bool {
    SCANNING.with(|s| s.replace(false))
}

pub fn take_cold_served() -> bool {
    S.cold_served.swap(false, Ordering::AcqRel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    fn line(id: &str) -> Line {
        Line {
            id: id.into(),
            ..Default::default()
        }
    }

    fn slow(runs: Arc<AtomicUsize>) -> Scan<'static> {
        Box::new(move || {
            std::thread::sleep(Duration::from_millis(300));
            let n = runs.fetch_add(1, Ordering::SeqCst);
            Ok(vec![line(&format!("l{n}"))])
        })
    }

    #[test]
    fn a_poke_ends_the_wait_early() {
        let t = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            poke();
        });
        let begin = Instant::now();
        assert!(wait_poke(Duration::from_secs(10)));
        assert!(begin.elapsed() < Duration::from_secs(5));
        t.join().unwrap();
        assert!(!wait_poke(Duration::from_millis(20)));
    }

    #[test]
    fn concurrent_scans_coalesce_and_cold_reads_do_not_block_forever() {
        let s: &'static Scanner = Box::leak(Box::default());
        let runs = Arc::new(AtomicUsize::new(0));
        let first = {
            let r = runs.clone();
            std::thread::spawn(move || s.fresh(slow(r)))
        };
        std::thread::sleep(Duration::from_millis(50));
        let begin = Instant::now();
        let cold = s
            .current(Duration::from_millis(50), Box::new(|| unreachable!()))
            .unwrap();
        assert!(cold.is_empty());
        assert!(take_scanning());
        assert!(s.cold_served.swap(false, Ordering::AcqRel));
        assert!(begin.elapsed() < Duration::from_millis(250));
        let waiters: Vec<_> = (0..8)
            .map(|_| {
                let r = runs.clone();
                std::thread::spawn(move || s.fresh(slow(r)))
            })
            .collect();
        assert_eq!(first.join().unwrap().unwrap()[0].id, "l0");
        for w in waiters {
            assert_eq!(w.join().unwrap().unwrap()[0].id, "l1");
        }
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        let warm = s.current(COLD_WAIT, Box::new(|| unreachable!())).unwrap();
        assert_eq!(warm[0].id, "l1");
        assert!(!take_scanning());
        let failed = s.fresh(Box::new(|| Err("db locked".into())));
        assert_eq!(failed.unwrap_err(), "db locked");
        let kept = s.current(COLD_WAIT, Box::new(|| unreachable!())).unwrap();
        assert_eq!(kept[0].id, "l1");
        let panicked = std::panic::catch_unwind(|| s.fresh(Box::new(|| panic!("boom"))));
        assert!(panicked.is_err());
        let after = s.fresh(Box::new(|| Ok(vec![line("after")]))).unwrap();
        assert_eq!(after[0].id, "after");
    }

    #[test]
    fn without_a_daemon_every_call_scans() {
        if shared() {
            return;
        }
        let runs = AtomicUsize::new(0);
        for _ in 0..3 {
            current(|| {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(vec![])
            })
            .unwrap();
        }
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }
}
