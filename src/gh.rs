use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(15);

type Cache = HashMap<(String, String), (Instant, Value)>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

pub fn ci(rollup: &Value) -> &'static str {
    let Some(items) = rollup.as_array().filter(|a| !a.is_empty()) else {
        return "none";
    };
    let up = |v: &Value| v.as_str().unwrap_or("").to_ascii_uppercase();
    let mut pending = false;
    for i in items {
        let (status, conclusion, state) = (up(&i["status"]), up(&i["conclusion"]), up(&i["state"]));
        let bad = [
            "FAILURE",
            "ERROR",
            "TIMED_OUT",
            "CANCELLED",
            "ACTION_REQUIRED",
            "STARTUP_FAILURE",
        ];
        if bad.contains(&conclusion.as_str()) || bad.contains(&state.as_str()) {
            return "failure";
        }
        if (!status.is_empty() && status != "COMPLETED")
            || matches!(state.as_str(), "PENDING" | "EXPECTED")
        {
            pending = true;
        }
    }
    if pending {
        "pending"
    } else {
        "success"
    }
}

fn fetch(repo: &str, branch: &str) -> Value {
    use crate::git::RunError;
    let out = match crate::git::run_within(
        Command::new("gh")
            .args(["pr", "list", "--state", "all", "--limit", "1"])
            .arg(format!("--repo={repo}"))
            .arg(format!("--head={branch}"))
            .arg("--json=number,url,state,isDraft,title,baseRefName,statusCheckRollup")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1"),
        TIMEOUT,
    ) {
        Ok(o) => o,
        Err(RunError::Spawn(e)) => {
            return json!({"pr": null, "error": format!("gh unavailable: {e}")})
        }
        Err(RunError::TimedOut(_)) => return json!({"pr": null, "error": "gh timed out"}),
        Err(RunError::Wait(e)) => return json!({"pr": null, "error": e.to_string()}),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return json!({"pr": null, "error": err.lines().next().unwrap_or("gh failed")});
    }
    let list: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    match list.get(0) {
        Some(p) => json!({"pr": {
            "number": p["number"], "url": p["url"], "state": p["state"], "isDraft": p["isDraft"],
            "title": p["title"], "base": p["baseRefName"], "ci": ci(&p["statusCheckRollup"]),
        }}),
        None => json!({"pr": null}),
    }
}

fn askable(repo: &str, branch: &str) -> bool {
    !(repo.is_empty()
        || branch.is_empty()
        || repo.starts_with('-')
        || repo.starts_with("local:")
        || repo.split('/').count() < 3)
}

pub fn pr_fresh(repo: &str, branch: &str) -> Value {
    if let Some(m) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(&(repo.to_string(), branch.to_string()));
    }
    pr_info(repo, branch)
}

pub fn pr_info(repo: &str, branch: &str) -> Value {
    if !askable(repo, branch) {
        return json!({"pr": null});
    }
    let key = (repo.to_string(), branch.to_string());
    if let Some((at, v)) = CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        if at.elapsed() < TTL {
            return v.clone();
        }
    }
    let v = fetch(repo, branch);
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, (Instant::now(), v.clone()));
    v
}

static INFLIGHT: Mutex<Vec<(String, String)>> = Mutex::new(vec![]);

pub fn cached_or_fetch(repo: &str, branch: &str, done: impl FnOnce() + Send + 'static) -> Value {
    if !askable(repo, branch) {
        return json!({"pr": null});
    }
    let key = (repo.to_string(), branch.to_string());
    if let Some((at, v)) = CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        if at.elapsed() < TTL {
            return v.clone();
        }
    }
    {
        let mut f = INFLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if f.contains(&key) {
            return json!({"pr": null, "pending": true});
        }
        f.push(key.clone());
    }
    std::thread::spawn(move || {
        pr_info(&key.0, &key.1);
        INFLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|k| *k != key);
        done();
    });
    json!({"pr": null, "pending": true})
}

#[cfg(test)]
mod tests {
    use super::ci;
    use serde_json::json;

    #[test]
    fn rollup_to_ci() {
        assert_eq!(ci(&json!([])), "none");
        assert_eq!(
            ci(&json!([{"status": "COMPLETED", "conclusion": "SUCCESS"}, {"state": "SUCCESS"}])),
            "success"
        );
        assert_eq!(
            ci(&json!([{"status": "IN_PROGRESS", "conclusion": ""}])),
            "pending"
        );
        assert_eq!(
            ci(&json!([{"status": "IN_PROGRESS"}, {"state": "FAILURE"}])),
            "failure"
        );
        assert_eq!(
            ci(&json!([{"status": "COMPLETED", "conclusion": "NEUTRAL"}, {"state": "PENDING"}])),
            "pending"
        );
    }
}
