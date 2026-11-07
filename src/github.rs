use crate::daemon::Request;
use crate::Ctx;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

type Res<T> = Result<T, (u16, String, &'static str)>;

const TTL: Duration = Duration::from_secs(60);
const GH_TIMEOUT: Duration = Duration::from_secs(30);
const CLONE_TIMEOUT: Duration = Duration::from_secs(600);
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
const WAIT: Duration = Duration::from_secs(20);
const SEARCH: &str = "is:pr is:open archived:false review-requested:@me";
const QUERY: &str = "query($q:String!){viewer{login} search(query:$q,type:ISSUE,first:100){nodes{... on PullRequest{number title url isDraft updatedAt headRefName baseRefName additions deletions changedFiles reviewDecision author{login} repository{nameWithOwner} reviews(last:50){nodes{author{login} state}}}}}}";

const MAX_PENDING: usize = 4;
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const JOURNAL: &str = "reviews-pending";
const RESUME_MAX_MS: i64 = 3_600_000;

static CACHE: Mutex<Option<(Instant, Value)>> = Mutex::new(None);
static FETCH: Mutex<()> = Mutex::new(());
static PREPARE: Mutex<()> = Mutex::new(());
static PENDING: AtomicUsize = AtomicUsize::new(0);
static CHILDREN: Mutex<Vec<u32>> = Mutex::new(Vec::new());

struct Child(u32);

impl Child {
    fn track(pid: u32) -> Child {
        CHILDREN.lock().unwrap_or_else(|e| e.into_inner()).push(pid);
        Child(pid)
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        CHILDREN
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| *p != self.0);
    }
}

pub fn before_exec() {
    #[cfg(unix)]
    for pid in CHILDREN.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        if let Ok(p) = i32::try_from(pid) {
            unsafe { libc::kill(-p, libc::SIGKILL) };
        }
    }
}

pub struct Pending(());

impl Pending {
    pub fn take() -> Option<Pending> {
        let mut n = PENDING.load(Ordering::Acquire);
        while n < MAX_PENDING {
            match PENDING.compare_exchange_weak(n, n + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(Pending(())),
                Err(now) => n = now,
            }
        }
        None
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn routes(ctx: &Ctx, r: &Request, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    let res = match (r.method.as_str(), segs) {
        ("GET", ["v1", "github", "review-requests"]) => {
            list(ctx, "gh", crate::daemon::q(r, "refresh") == Some("1")).map(|v| (200, v))
        }
        ("POST", ["v1", "github", "review-requests", owner, repo, number, "review"]) => {
            review(ctx, owner, repo, number, body)
        }
        _ => return None,
    };
    Some(res.unwrap_or_else(|(c, e, code)| (c, json!({ "error": e, "code": code }))))
}

pub fn run(bin: &str, dir: Option<&Path>, args: &[&str], timeout: Duration) -> Res<String> {
    let mut cmd = Command::new(bin);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                (
                    424,
                    format!("{bin} is not installed on this host"),
                    "missing",
                )
            } else {
                (500, format!("{bin}: {e}"), "failed")
            }
        })?;
    fn drain(p: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut s = Vec::new();
            if let Some(mut p) = p {
                let _ = p.read_to_end(&mut s);
            }
            s
        })
    }
    let _tracked = Child::track(child.id());
    let (out, err) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let begin = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if begin.elapsed() > timeout => {
                #[cfg(unix)]
                if let Ok(pid) = i32::try_from(child.id()) {
                    unsafe { libc::kill(-pid, libc::SIGKILL) };
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err((504, format!("{bin} {} timed out", args[0]), "timeout"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err((500, e.to_string(), "failed")),
        }
    };
    let (stdout, stderr) = (
        out.join().unwrap_or_default(),
        err.join().unwrap_or_default(),
    );
    if status.success() {
        return Ok(String::from_utf8_lossy(&stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&stderr);
    let first = err
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("failed");
    Err((
        502,
        crate::dlog::redact(&format!("{bin} {}: {first}", args[0])),
        "failed",
    ))
}

pub fn fetch(bin: &str) -> Res<Value> {
    let q = format!("q={SEARCH}");
    let query = format!("query={QUERY}");
    let out = run(
        bin,
        None,
        &["api", "graphql", "-f", &q, "-f", &query],
        GH_TIMEOUT,
    )
    .map_err(|e| {
        if e.2 != "failed" {
            return e;
        }
        if run(bin, None, &["auth", "token"], AUTH_TIMEOUT).is_err() {
            (424, "gh is not logged in on this host".to_string(), "auth")
        } else {
            (
                e.0,
                format!("GitHub could not be reached: {}", e.1),
                "failed",
            )
        }
    })?;
    let v: Value =
        serde_json::from_str(&out).map_err(|e| (502, format!("gh api graphql: {e}"), "failed"))?;
    Ok(parse(&v))
}

pub fn parse(v: &Value) -> Value {
    let login = v["data"]["viewer"]["login"].as_str().unwrap_or("");
    let prs: Vec<Value> = v["data"]["search"]["nodes"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|p| p["number"].is_u64())
        .map(|p| {
            let mine: Vec<&str> = p["reviews"]["nodes"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or_default()
                .iter()
                .filter(|r| {
                    !login.is_empty()
                        && r["author"]["login"].as_str() == Some(login)
                        && r["state"] != "PENDING"
                })
                .filter_map(|r| r["state"].as_str())
                .collect();
            json!({
                "repo": p["repository"]["nameWithOwner"],
                "number": p["number"],
                "title": p["title"],
                "url": p["url"],
                "author": p["author"]["login"],
                "headRefName": p["headRefName"],
                "baseRefName": p["baseRefName"],
                "updatedAt": p["updatedAt"],
                "isDraft": p["isDraft"].as_bool().unwrap_or(false),
                "additions": p["additions"].as_u64().unwrap_or(0),
                "deletions": p["deletions"].as_u64().unwrap_or(0),
                "changedFiles": p["changedFiles"].as_u64().unwrap_or(0),
                "reviewDecision": p["reviewDecision"],
                "reviewed": !mine.is_empty(),
                "myState": mine.last(),
            })
        })
        .collect();
    json!({ "login": login, "prs": prs })
}

pub fn local_for(projects: &[Value], repo: &str) -> Option<String> {
    let want = format!("github.com/{repo}").to_ascii_lowercase();
    projects
        .iter()
        .filter(|p| p["missing"] != true)
        .find(|p| p["repo"].as_str().map(str::to_ascii_lowercase).as_deref() == Some(&want))
        .and_then(|p| p["path"].as_str())
        .map(String::from)
}

fn projects(ctx: &Ctx) -> Vec<Value> {
    crate::tasks::projects(ctx, &crate::daemon::lines(ctx)).unwrap_or_default()
}

fn fresh(since: impl Fn(Instant) -> bool) -> Option<Value> {
    let c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    c.as_ref()
        .filter(|(at, _)| since(*at))
        .map(|(_, v)| v.clone())
}

fn list(ctx: &Ctx, bin: &str, refresh: bool) -> Res<Value> {
    let asked = Instant::now();
    if !refresh {
        if let Some(v) = fresh(|at| at.elapsed() < TTL) {
            return Ok(v);
        }
    }
    let _one = FETCH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = fresh(|at| at >= asked) {
        return Ok(v);
    }
    let mut v = fetch(bin)?;
    let projects = projects(ctx);
    if let Some(prs) = v["prs"].as_array_mut() {
        for p in prs {
            let local = local_for(&projects, p["repo"].as_str().unwrap_or(""));
            p["localPath"] = json!(local);
        }
    }
    v["fetchedAt"] = json!(crate::procs::now_ms());
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), v.clone()));
    Ok(v)
}

fn cached(repo: &str, number: u64) -> Option<Value> {
    let c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    c.as_ref()?.1["prs"]
        .as_array()?
        .iter()
        .find(|p| {
            p["number"] == number
                && p["repo"]
                    .as_str()
                    .is_some_and(|r| r.eq_ignore_ascii_case(repo))
        })
        .cloned()
}

pub struct Prepared {
    pub root: String,
    pub worktree: String,
    pub branch: String,
}

fn git(dir: &str, args: &[&str], timeout: Duration) -> Res<String> {
    run("git", Some(Path::new(dir)), args, timeout)
}

fn clones_dir(repo: &str, legacy: &Path, current: &Path) -> PathBuf {
    if !current.join(repo).join(".git").exists() && legacy.join(repo).join(".git").exists() {
        return legacy.to_path_buf();
    }
    current.to_path_buf()
}

pub fn prepare(
    ctx: &Ctx,
    bin: &str,
    clones: &Path,
    local: Option<String>,
    repo: &str,
    number: u64,
) -> Res<Prepared> {
    let _g = PREPARE.lock().unwrap_or_else(|e| e.into_inner());
    let root = match local.filter(|p| Path::new(p).is_dir()) {
        Some(p) => p,
        None => {
            let dir: PathBuf = clones.join(repo);
            if !dir.join(".git").exists() {
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(dir.parent().unwrap_or(clones))
                    .map_err(|e| (500, format!("{}: {e}", clones.display()), "failed"))?;
                let mut part = dir.clone().into_os_string();
                part.push(".partial");
                let part = PathBuf::from(part);
                let _ = std::fs::remove_dir_all(&part);
                let d = part.to_string_lossy();
                let cloned = run(
                    bin,
                    None,
                    &[
                        "repo",
                        "clone",
                        repo,
                        &d,
                        "--",
                        "--filter=blob:none",
                        "--quiet",
                    ],
                    CLONE_TIMEOUT,
                )
                .and_then(|_| {
                    std::fs::rename(&part, &dir)
                        .map_err(|e| (500, format!("{}: {e}", dir.display()), "failed"))
                });
                if cloned.is_err() {
                    let _ = std::fs::remove_dir_all(&part);
                }
                cloned?;
            }
            dir.to_string_lossy().into_owned()
        }
    };
    let (root, _) = crate::worktrees::project(&root).map_err(|(c, e)| (c, e, "failed"))?;
    let pull = format!("pull/{number}/head");
    let branch = format!("review/pr-{number}");
    let dir = crate::worktrees::store(ctx, &root).join(crate::worktrees::dir_name(&branch));
    if dir.is_dir() {
        let d = dir.to_string_lossy().into_owned();
        if git(
            &d,
            &["fetch", "--quiet", "--no-tags", "origin", &pull],
            FETCH_TIMEOUT,
        )
        .is_ok()
        {
            let _ = git(
                &d,
                &["merge", "--ff-only", "--quiet", "FETCH_HEAD"],
                FETCH_TIMEOUT,
            );
        }
        let path = dir.canonicalize().unwrap_or(dir);
        return Ok(Prepared {
            root,
            worktree: path.to_string_lossy().into_owned(),
            branch,
        });
    }
    git(
        &root,
        &["fetch", "--quiet", "--no-tags", "origin", &pull],
        FETCH_TIMEOUT,
    )?;
    let sha = git(&root, &["rev-parse", "FETCH_HEAD"], FETCH_TIMEOUT)?
        .trim()
        .to_string();
    if crate::git::git(
        &root,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_some()
    {
        git(&root, &["branch", "-f", &branch, &sha], FETCH_TIMEOUT)?;
    }
    let (root, worktree, branch) = crate::worktrees::create(
        ctx,
        &root,
        &crate::worktrees::New {
            branch: Some(&branch),
            base: Some(&sha),
            ..Default::default()
        },
    )
    .map_err(|(c, e)| (c, e, "failed"))?;
    crate::worktrees::announce(root.clone(), worktree.clone());
    Ok(Prepared {
        root,
        worktree,
        branch,
    })
}

pub fn prompt(repo: &str, number: u64, pr: Option<&Value>) -> String {
    let s = |k: &str| pr.and_then(|p| p[k].as_str()).unwrap_or("");
    let mut head = format!("You are reviewing GitHub pull request #{number} in {repo}");
    if !s("title").is_empty() {
        head.push_str(&format!(" \"{}\"", s("title")));
    }
    if !s("author").is_empty() {
        head.push_str(&format!(" by @{}", s("author")));
    }
    let base = match s("baseRefName") {
        "" => "the base branch".to_string(),
        b => format!("origin/{b}"),
    };
    format!(
        "{head}. The current directory is a worktree checked out at the PR head.\n\n\
1. Read the description and the discussion: gh pr view {number} --repo {repo} --comments\n\
2. Read the diff against {base}: gh pr diff {number} --repo {repo} (or git diff against the merge base), and open the changed files for context.\n\
3. Review it for correctness, security, error handling, performance and tests. Run the relevant tests when it is cheap.\n\
4. Write your findings here, grouped by severity, each with file:line and a concrete suggestion, then propose a verdict: comment, request changes or approve.\n\n\
Then stop and wait for me. Do not post anything to GitHub, do not approve, request changes or comment, and do not commit, push or merge until I explicitly confirm in this chat what to post. Never approve on your own initiative.\n\n\
Only after my explicit go-ahead, post exactly what I agreed to: the summary with gh pr review {number} --repo {repo} --comment|--request-changes|--approve --body-file <file>, and inline comments with gh api repos/{repo}/pulls/{number}/comments -f body=<text> -f commit_id=<head sha> -f path=<file> -F line=<line> -f side=RIGHT."
    )
}

fn review(ctx: &Ctx, owner: &str, name: &str, number: &str, b: &Value) -> Res<(u16, Value)> {
    let repo = format!("{owner}/{name}");
    if !crate::feedback::valid_repo(&repo) {
        return Err((400, "repo must look like owner/name".into(), "bad"));
    }
    let number: u64 = number.parse().ok().filter(|n| *n > 0).ok_or((
        400,
        "number must be a positive integer".into(),
        "bad",
    ))?;
    let agent = crate::feedback::agent_for(b).map_err(|(c, e)| (c, e, "bad"))?;
    let pr = cached(&repo, number);
    let local = pr
        .as_ref()
        .and_then(|p| p["localPath"].as_str().map(String::from))
        .or_else(|| local_for(&projects(ctx), &repo));
    let title = format!("PR #{number} {repo}");
    let Some(slot) = Pending::take() else {
        return Err((
            429,
            "too many PR reviews are being prepared; try again shortly".into(),
            "busy",
        ));
    };
    let job = json!({
        "title": title,
        "repo": repo,
        "number": number,
        "body": b,
        "local": local,
        "pr": pr,
        "startedAt": crate::procs::now_ms(),
    });
    let dir = crate::default_state_dir().join(JOURNAL);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _slot = slot;
        let _ = tx.send(run_job(&dir, &job, agent));
    });
    match rx.recv_timeout(WAIT) {
        Ok(r) => r.map(|v| (201, v)),
        Err(_) => Ok((202, json!({ "pending": true, "title": title }))),
    }
}

fn journal_path(dir: &Path, repo: &str, number: u64) -> PathBuf {
    dir.join(format!("{}-{number}.json", repo.replace('/', "__")))
}

fn failed(job: &Value, e: &str) {
    let title = job["title"].as_str().unwrap_or("PR review");
    crate::dlog::line("warn", &format!("review {title}: {e}"));
    crate::board::publish(
        "review.failed",
        json!({"title": title, "repo": job["repo"], "number": job["number"], "error": e}),
    );
}

fn run_job(dir: &Path, job: &Value, agent: &'static crate::agents::Agent) -> Res<Value> {
    let repo = job["repo"].as_str().unwrap_or_default().to_string();
    let number = job["number"].as_u64().unwrap_or_default();
    let title = job["title"].as_str().unwrap_or_default().to_string();
    let path = journal_path(dir, &repo, number);
    let _ = crate::mkdir_private(dir).and_then(|_| std::fs::write(&path, job.to_string()));
    let launched = (job["resumed"] == true)
        .then(|| {
            crate::term::all()
                .into_iter()
                .find(|t| t.info().title == title && !t.info().exited)
        })
        .flatten();
    let r = match launched {
        Some(t) => Ok(json!({"term": t.to_json(), "worktree": t.info().cwd})),
        None => {
            let probe = crate::procs::System;
            let ctx = Ctx::from_env(&probe);
            prepare(
                &ctx,
                "gh",
                &clones_dir(
                    &repo,
                    &crate::storage::legacy_reviews_dir(),
                    &crate::storage::reviews_dir(&crate::default_state_dir()),
                ),
                job["local"].as_str().map(String::from),
                &repo,
                number,
            )
            .and_then(|p| {
                let pr = Some(&job["pr"]).filter(|v| !v.is_null());
                let prompt = prompt(&repo, number, pr);
                crate::feedback::launch(
                    &ctx,
                    agent,
                    &job["body"],
                    p.worktree.clone(),
                    prompt,
                    &title,
                )
                .map_err(|(c, e)| (c, e, "failed"))
                .map(|term| {
                    json!({
                        "term": term,
                        "worktree": p.worktree,
                        "project": p.root,
                        "branch": p.branch,
                    })
                })
            })
        }
    };
    let _ = std::fs::remove_file(&path);
    if let Err((_, e, _)) = &r {
        failed(job, e);
    }
    r
}

pub fn resume_pending(state_dir: &Path) {
    let dir = state_dir.join(JOURNAL);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return;
    };
    let now = crate::procs::now_ms();
    for e in rd.flatten() {
        let path = e.path();
        let job: Value = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let _ = std::fs::remove_file(&path);
        if !job.is_object() {
            continue;
        }
        let fresh = job["startedAt"]
            .as_i64()
            .is_some_and(|t| now - t <= RESUME_MAX_MS);
        let agent = crate::feedback::agent_for(&job["body"]).ok();
        let (Some(agent), true) = (agent, fresh) else {
            failed(&job, LOST);
            continue;
        };
        let Some(slot) = Pending::take() else {
            failed(&job, LOST);
            continue;
        };
        let mut job = job;
        job["resumed"] = json!(true);
        let dir = dir.clone();
        std::thread::spawn(move || {
            let _slot = slot;
            let _ = run_job(&dir, &job, agent);
        });
    }
}

const LOST: &str = "the daemon restarted while this review was being prepared; start it again";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_waits_for_confirmation() {
        let p = prompt(
            "acme/api",
            7,
            Some(&json!({"title": "Fix", "author": "bob", "baseRefName": "main"})),
        );
        assert!(p.contains("#7 in acme/api \"Fix\" by @bob"));
        assert!(p.contains("gh pr diff 7 --repo acme/api"));
        assert!(p.contains("origin/main"));
        assert!(p.contains("Never approve on your own initiative"));
        assert!(p.contains("until I explicitly confirm"));
        assert!(prompt("acme/api", 7, None).contains("the base branch"));
    }

    #[test]
    fn existing_clones_stay_where_they_are_and_new_ones_use_the_state_dir() {
        let d = std::env::temp_dir().join(format!("sb-clones-{}", std::process::id()));
        let (old, new) = (d.join("old"), d.join("new"));
        std::fs::create_dir_all(old.join("o/kept/.git")).unwrap();
        assert_eq!(clones_dir("o/kept", &old, &new), old);
        assert_eq!(clones_dir("o/fresh", &old, &new), new);
        std::fs::create_dir_all(new.join("o/kept/.git")).unwrap();
        assert_eq!(clones_dir("o/kept", &old, &new), new);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn reviews_cut_by_a_restart_are_reported_not_lost() {
        let d = std::env::temp_dir().join(format!("sb-gh-journal-{}", std::process::id()));
        let dir = d.join(JOURNAL);
        std::fs::create_dir_all(&dir).unwrap();
        let now = crate::procs::now_ms();
        let jobs = [
            json!({"title": "PR #1 acme/old", "repo": "acme/old", "number": 1, "body": {"agent": "claude"}, "startedAt": now - 2 * RESUME_MAX_MS}),
            json!({"title": "PR #2 acme/bad", "repo": "acme/bad", "number": 2, "body": {"agent": "nope"}, "startedAt": now}),
        ];
        for j in &jobs {
            let p = journal_path(
                &dir,
                j["repo"].as_str().unwrap(),
                j["number"].as_u64().unwrap(),
            );
            std::fs::write(p, j.to_string()).unwrap();
        }
        std::fs::write(dir.join("junk.json"), "{").unwrap();
        let (rx, _) = crate::board::subscribe(None);
        resume_pending(&d);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let mut seen = std::collections::HashSet::new();
        while seen.len() < 2 {
            let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let data = ev.lines().find_map(|l| l.strip_prefix("data: ")).unwrap();
            let v: Value = serde_json::from_str(data).unwrap();
            if v["type"] == "review.failed"
                && v["data"]["repo"]
                    .as_str()
                    .is_some_and(|r| r.starts_with("acme/"))
            {
                assert_eq!(v["data"]["error"], LOST);
                seen.insert(v["data"]["number"].as_u64().unwrap());
            }
        }
        assert_eq!(seen, [1, 2].into_iter().collect());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn pending_reviews_are_capped() {
        let held: Vec<Pending> = std::iter::from_fn(Pending::take).take(64).collect();
        assert_eq!(held.len(), MAX_PENDING);
        assert!(Pending::take().is_none());
        drop(held);
        assert!(Pending::take().is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn timeout_kills_the_whole_process_group() {
        let d = std::env::temp_dir().join(format!("sb-gh-run-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let pid = d.join("pid");
        let script = format!("sleep 300 & echo $! > {}; wait", pid.display());
        let begin = Instant::now();
        let e = run(
            "/bin/sh",
            None,
            &["-c", &script],
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert_eq!((e.0, e.2), (504, "timeout"));
        assert!(begin.elapsed() < Duration::from_secs(5));
        let child: u32 = std::fs::read_to_string(&pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let gone = (0..100).any(|_| {
            let state = std::fs::read_to_string(format!("/proc/{child}/stat")).unwrap_or_default();
            let dead = state.is_empty() || state.contains(") Z ");
            if !dead {
                std::thread::sleep(Duration::from_millis(20));
            }
            dead
        });
        assert!(gone, "sleep {child} outlived the timeout");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn local_clone_lookup() {
        let projects = vec![
            json!({"path": "/gone", "repo": "github.com/acme/api", "missing": true}),
            json!({"path": "/src/api", "repo": "github.com/Acme/API"}),
        ];
        assert_eq!(
            local_for(&projects, "acme/api").as_deref(),
            Some("/src/api")
        );
        assert_eq!(local_for(&projects, "acme/web"), None);
    }
}
