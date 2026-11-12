use crate::board::publish;
use crate::deliver::{self, Channel, Job, NotIdle, Queue, Rules};
use crate::procs::now_ms;
use crate::registry::db;
use crate::term;
use crate::util::ulid;
use crate::workspaces::{bad, Index, Res};
use crate::{Ctx, Line};
use serde_json::{json, Value};

const QUEUE_TTL_MS: i64 = 3_600_000;
const MAX_TEXT: usize = 64 << 10;

enum Check {
    Idle(Channel),
    Busy(Channel, &'static str),
    Refused(&'static str),
}

static QUEUE: Queue = Queue::new();

fn verdict(l: Option<&Line>, ch: Channel) -> Check {
    match deliver::not_idle(l, &ch, None, Rules::default()) {
        Some(NotIdle::NeedsInput) => Check::Busy(ch, "needs input"),
        Some(_) => Check::Busy(ch, "busy"),
        None => Check::Idle(ch),
    }
}

fn check(target: &str, lines: &[Line]) -> Check {
    match target.split_once(':') {
        Some(("line", id)) => {
            let Some(l) = lines.iter().find(|l| l.id == id) else {
                return Check::Refused("unknown line");
            };
            if !l.alive()
                || matches!(
                    l.state,
                    crate::wire::STATE_HELD | crate::wire::STATE_HUNG_UP
                )
            {
                return Check::Refused("not running");
            }
            match deliver::channel(l) {
                Some(ch) => verdict(Some(l), ch),
                None => Check::Refused("no terminal"),
            }
        }
        Some(("term", id)) => {
            let Some(t) = term::get(id) else {
                return Check::Refused("unknown term");
            };
            if t.exited() {
                return Check::Refused("exited");
            }
            let own = t.info().line_id;
            let l = lines.iter().filter(|l| l.alive()).find(|l| {
                l.term_id.as_deref() == Some(id) || own.as_deref() == Some(l.id.as_str())
            });
            verdict(l, Channel::Term(t))
        }
        _ => Check::Refused("unknown target"),
    }
}

fn local(ctx: &Ctx, lines: Vec<Line>) -> Vec<Line> {
    lines
        .into_iter()
        .filter(|l| l.station_id == ctx.config.station_id)
        .collect()
}

fn opt_bool(b: &Value, k: &str, default: bool) -> Res<bool> {
    match &b[k] {
        Value::Null => Ok(default),
        Value::Bool(x) => Ok(*x),
        _ => Err(bad(format!("{k} must be a boolean"))),
    }
}

fn ids(b: &Value, k: &str, kind: &str) -> Res<Vec<String>> {
    match &b[k] {
        Value::Null => Ok(vec![]),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map(|s| format!("{kind}:{s}")))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| bad(format!("{k} must be an array of strings"))),
        _ => Err(bad(format!("{k} must be an array of strings"))),
    }
}

fn targets(ctx: &Ctx, b: &Value, lines: &[Line]) -> Res<Vec<String>> {
    let by_ids = !b["lineIds"].is_null() || !b["termIds"].is_null();
    let sources = [by_ids, !b["workspaceId"].is_null(), !b["groupId"].is_null()];
    if sources.iter().filter(|s| **s).count() != 1 {
        return Err(bad(
            "give exactly one source: lineIds/termIds, workspaceId or groupId",
        ));
    }
    let mut out = if by_ids {
        let mut v = ids(b, "lineIds", "line")?;
        v.extend(ids(b, "termIds", "term")?);
        v
    } else if let Some(gid) = b["groupId"].as_str() {
        crate::workspaces::get_group(&db(ctx)?, gid)?.members
    } else {
        let wid = b["workspaceId"]
            .as_str()
            .ok_or_else(|| bad("workspaceId must be a string"))?;
        crate::workspaces::get(&db(ctx)?, wid)?;
        let ix = Index::new(ctx, lines)?;
        let mut v: Vec<String> = lines
            .iter()
            .filter(|l| l.alive() && ix.of_line(l).is_some_and(|w| w.id == wid))
            .map(|l| format!("line:{}", l.id))
            .collect();
        v.extend(
            term::all()
                .iter()
                .map(|t| t.info())
                .filter(|i| !i.exited && i.line_id.is_none())
                .filter(|i| !lines.iter().any(|l| l.term_id.as_ref() == Some(&i.id)))
                .filter(|i| ix.of_term(i).is_some_and(|w| w.id == wid))
                .map(|i| format!("term:{}", i.id)),
        );
        v
    };
    let mut seen = std::collections::HashSet::new();
    out.retain(|t| seen.insert(t.clone()));
    Ok(out)
}

pub fn post(ctx: &Ctx, b: &Value) -> Res<Value> {
    if !b.is_object() {
        return Err(bad("JSON object expected"));
    }
    let text = b["text"].as_str().map(str::trim).unwrap_or_default();
    if text.is_empty() {
        return Err(bad("text required"));
    }
    if text.len() > MAX_TEXT {
        return Err(bad("text must be at most 64 KiB"));
    }
    let only_idle = opt_bool(b, "onlyIdle", true)?;
    let queue = opt_bool(b, "queue", false)?;
    let lines = local(
        ctx,
        crate::daemon::cached_lines(ctx, None).map_err(|e| (500, e))?,
    );
    let targets = targets(ctx, b, &lines)?;
    let id = ulid(now_ms());
    let mut results: Vec<Value> = vec![];
    let mut jobs = vec![];
    for (i, target) in targets.iter().enumerate() {
        let mut r = json!({"target": target});
        let send = match check(target, &lines) {
            Check::Refused(why) => {
                r["result"] = json!("refused");
                r["reason"] = json!(why);
                None
            }
            Check::Idle(ch) => Some(ch),
            Check::Busy(ch, _) if !only_idle => Some(ch),
            Check::Busy(_, why) if queue => {
                r["result"] = json!("queued");
                r["reason"] = json!(why);
                QUEUE.push(Job {
                    id: id.clone(),
                    target: target.clone(),
                    text: text.to_string(),
                    at_ms: now_ms(),
                });
                None
            }
            Check::Busy(_, why) => {
                r["result"] = json!("skipped");
                r["reason"] = json!(why);
                None
            }
        };
        if let Some(ch) = send {
            let (text, tmux, buffer) = (text.to_string(), ctx.tmux.clone(), format!("sb-{id}-{i}"));
            jobs.push((
                results.len(),
                std::thread::spawn(move || deliver::send(&ch, &text, tmux.as_ref(), &buffer)),
            ));
        }
        results.push(r);
    }
    for (at, job) in jobs {
        match job
            .join()
            .unwrap_or_else(|_| Err("delivery panicked".into()))
        {
            Ok(()) => results[at]["result"] = json!("sent"),
            Err(e) => {
                results[at]["result"] = json!("refused");
                results[at]["reason"] = json!(e);
            }
        }
    }
    crate::actions::log_action(
        ctx,
        &json!({"action": "broadcast", "id": id, "chars": text.chars().count(), "results": results}),
    );
    Ok(json!({"id": id, "results": results}))
}

fn delivered(q: &Job, result: &str, reason: Option<&str>) {
    let mut v = json!({"id": q.id, "target": q.target, "result": result});
    if let Some(r) = reason {
        v["reason"] = json!(r);
    }
    publish("broadcast.delivered", v);
}

pub fn saved() -> Vec<Job> {
    QUEUE.snapshot()
}

pub fn restore(jobs: Vec<Job>) {
    for j in jobs {
        QUEUE.push(j);
    }
}

pub fn tick(ctx: &Ctx, lines: &[Line]) {
    let lines = local(ctx, lines.to_vec());
    let now = now_ms();
    let mut i = 0;
    QUEUE.drain(|q| {
        i += 1;
        if now - q.at_ms > QUEUE_TTL_MS {
            delivered(q, "dropped", Some("expired"));
            return false;
        }
        match check(&q.target, &lines) {
            Check::Refused(why) => delivered(q, "dropped", Some(why)),
            Check::Busy(..) => return true,
            Check::Idle(ch) => {
                let tmux = ctx.tmux.clone();
                let buffer = format!("sb-{}-q{i}", q.id);
                let q = Job {
                    id: q.id.clone(),
                    target: q.target.clone(),
                    text: q.text.clone(),
                    at_ms: q.at_ms,
                };
                std::thread::spawn(move || {
                    match deliver::send(&ch, &q.text, tmux.as_ref(), &buffer) {
                        Ok(()) => delivered(&q, "sent", None),
                        Err(e) => delivered(&q, "dropped", Some(e.as_str())),
                    }
                });
            }
        }
        false
    });
}
