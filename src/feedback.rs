use crate::sys::OpenOptionsExt;
use crate::Ctx;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

type Res<T> = Result<T, (u16, String)>;

pub const REPORT: &str = "@REPORT@";
const MAX_BODY: usize = 60_000;
const MAX_PROMPT: usize = 16_000;
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const GH_TIMEOUT: Duration = Duration::from_secs(45);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_FILE: u64 = 25 * 1024 * 1024;
const MAX_TOTAL: u64 = 50 * 1024 * 1024;
const BRANCH: &str = "overseer-attachments";
pub const ATTACHMENTS: &str = "@ATTACHMENTS@";
const IMAGES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
const POSITIONAL: [&str; 2] = ["claude", "codex"];

pub fn routes(ctx: &Ctx, method: &str, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    let r = match (method, segs) {
        ("POST", ["v1", "feedback", "start"]) => start(ctx, body).map(|v| (201, v)),
        ("POST", ["v1", "feedback", "direct"]) => direct(ctx, body).map(|v| (201, v)),
        _ => return None,
    };
    Some(r.unwrap_or_else(|(c, e)| (c, json!({ "error": e }))))
}

fn bad(m: impl Into<String>) -> (u16, String) {
    (400, m.into())
}

fn text<'a>(b: &'a Value, k: &str) -> &'a str {
    b[k].as_str().map(str::trim).unwrap_or("")
}

pub fn valid_repo(r: &str) -> bool {
    let mut parts = r.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && !p.starts_with(['-', '.'])
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

fn checked_body(b: &Value) -> Res<&str> {
    let body = text(b, "body");
    if body.is_empty() {
        return Err(bad("body required"));
    }
    if body.len() > MAX_BODY {
        return Err(bad(format!("body is longer than {MAX_BODY} bytes")));
    }
    Ok(body)
}

fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("feedback")
}

pub fn write_report(state_dir: &Path, body: &str, now: SystemTime) -> Res<PathBuf> {
    let d = dir(state_dir);
    crate::mkdir_private(&d).map_err(|e| (500, format!("{}: {e}", d.display())))?;
    prune(&d, now);
    let ms = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut n = 0;
    loop {
        let p = d.join(format!("report-{ms}-{n}.md"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&p)
        {
            Ok(mut f) => {
                f.write_all(body.as_bytes())
                    .map_err(|e| (500, format!("{}: {e}", p.display())))?;
                return Ok(std::path::absolute(&p).unwrap_or(p));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 16 => n += 1,
            Err(e) => return Err((500, format!("{}: {e}", p.display()))),
        }
    }
}

fn prune(d: &Path, now: SystemTime) {
    let Ok(rd) = std::fs::read_dir(d) else {
        return;
    };
    for f in rd.flatten() {
        let name = f.file_name();
        let old = f
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|m| now.duration_since(m).is_ok_and(|a| a > KEEP));
        if old && name.to_string_lossy().starts_with("report-") {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

pub fn workdir(state_dir: &Path, cwd: &str) -> Res<String> {
    if !cwd.is_empty() && Path::new(cwd).is_dir() {
        return Ok(cwd.to_string());
    }
    let w = dir(state_dir).join("work");
    crate::mkdir_private(&w).map_err(|e| (500, format!("{}: {e}", w.display())))?;
    let w = std::path::absolute(&w).unwrap_or(w);
    Ok(w.to_string_lossy().into_owned())
}

pub fn fill(prompt: &str, report: &Path) -> String {
    prompt.replace(REPORT, &report.to_string_lossy())
}

fn start(ctx: &Ctx, b: &Value) -> Res<Value> {
    let body = checked_body(b)?;
    let prompt = text(b, "prompt");
    if !prompt.contains(REPORT) {
        return Err(bad(format!("prompt must reference {REPORT}")));
    }
    if prompt.len() > MAX_PROMPT {
        return Err(bad("prompt is too long"));
    }
    let agent = agent_for(b)?;
    let cwd = workdir(&ctx.state_dir, text(b, "cwd"))?;
    let body = with_attachments(ctx, b, text(b, "repo"), body)?;
    let report = write_report(&ctx.state_dir, &body, SystemTime::now())?;
    let term = launch(ctx, agent, b, cwd, fill(prompt, &report), "Feedback")?;
    Ok(json!({ "term": term, "report": report.to_string_lossy() }))
}

pub fn agent_for(b: &Value) -> Res<&'static crate::agents::Agent> {
    let id = text(b, "agent");
    let agent = crate::agents::get(id).ok_or_else(|| bad(format!("unknown agent {id}")))?;
    if agent.bin.is_empty() {
        return Err(bad("pick an agent CLI, not a shell"));
    }
    if !crate::agents::is_installed(agent) {
        return Err((
            409,
            format!("{} is not installed ({})", agent.name, agent.bin),
        ));
    }
    let effort = text(b, "effort");
    if !effort.is_empty() && !crate::agents::EFFORTS.contains(&effort) {
        return Err(bad(format!(
            "effort must be one of {}",
            crate::agents::EFFORTS.join(", ")
        )));
    }
    Ok(agent)
}

pub fn launch(
    ctx: &Ctx,
    agent: &crate::agents::Agent,
    b: &Value,
    cwd: String,
    prompt: String,
    title: &str,
) -> Res<Value> {
    let (model, effort) = (text(b, "model"), text(b, "effort"));
    let (cols, rows) = crate::term::size(b).map_err(bad)?.unwrap_or((120, 32));
    let session = agent.session.and_then(|_| crate::runner::uuid().ok());
    let by_flag = agent.prompt.is_some();
    let by_arg = !by_flag && POSITIONAL.contains(&agent.id);
    let extra: &[String] = if agent.id == "claude" {
        &ctx.config.start_args
    } else {
        &[]
    };
    let mut cmd = agent.launch(&crate::agents::Launch {
        model,
        effort,
        session: session.as_deref(),
        prompt: by_flag.then_some(prompt.as_str()),
        extra,
    });
    let shown = if by_arg {
        let shown = with_prompt(&cmd, "<prompt>");
        cmd = with_prompt(&cmd, &prompt);
        Some(shown)
    } else {
        by_flag.then(|| crate::agents::redact(&cmd, agent, Some(&prompt)))
    };
    let term = crate::term::spawn(crate::term::Spec {
        cwd,
        shown,
        cmd,
        cols,
        rows,
        title: Some(title.into()),
        session,
        flags: agent.flags(model, effort),
        ..Default::default()
    })
    .map_err(|e| (409, e))?;
    let json = term.to_json();
    if !by_flag && !by_arg {
        std::thread::spawn(move || crate::deliver::when_started(&term, &prompt, false));
    }
    Ok(json)
}

pub fn with_prompt(cmd: &[String], prompt: &str) -> Vec<String> {
    let mut v = cmd.to_vec();
    v.push(prompt.to_string());
    v
}

pub fn issue_url(out: &str) -> Option<String> {
    out.split_whitespace().rev().find_map(|w| {
        let w = w.trim_matches(|c: char| "<>()[]\"'.,".contains(c));
        let rest = w.strip_prefix("https://github.com/")?;
        let p: Vec<&str> = rest.split('/').collect();
        let ok = p.len() == 4
            && valid_repo(&format!("{}/{}", p[0], p[1]))
            && p[2] == "issues"
            && !p[3].is_empty()
            && p[3].bytes().all(|c| c.is_ascii_digit());
        ok.then(|| w.to_string())
    })
}

pub fn labels(b: &Value) -> Vec<String> {
    b["labels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .filter(|l| {
                    !l.is_empty()
                        && l.len() <= 50
                        && !l.starts_with('-')
                        && !l.contains([',', '\n'])
                })
                .take(5)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn label_problem(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("label")
}

pub fn args(repo: &str, title: &str, body_file: &Path, labels: &[String]) -> Vec<String> {
    let mut a = vec![
        "issue".to_string(),
        "create".into(),
        format!("--repo={repo}"),
        format!("--title={title}"),
        format!("--body-file={}", body_file.to_string_lossy()),
    ];
    a.extend(labels.iter().map(|l| format!("--label={l}")));
    a
}

fn gh(args: &[String]) -> Res<(bool, String, String)> {
    gh_within(args, GH_TIMEOUT)
}

fn gh_within(args: &[String], limit: Duration) -> Res<(bool, String, String)> {
    use crate::git::RunError;
    let out = crate::git::run_within(
        Command::new("gh")
            .args(args)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1"),
        limit,
    )
    .map_err(|e| match e {
        RunError::Spawn(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (424, "gh is not installed on this host".to_string())
        }
        RunError::Spawn(e) => (500, format!("gh: {e}")),
        RunError::TimedOut(_) => (504, "gh timed out".into()),
        RunError::Wait(e) => (500, e.to_string()),
    })?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

fn with_attachments(ctx: &Ctx, b: &Value, repo: &str, body: &str) -> Res<String> {
    let files =
        crate::uploads::attachments(&crate::uploads::dir(), &b["attachments"]).map_err(bad)?;
    if files.is_empty() {
        return Ok(body.replace(ATTACHMENTS, "(none)"));
    }
    if !valid_repo(repo) {
        return Err(bad("repo must look like owner/name"));
    }
    let results = upload_all(ctx, repo, &files);
    Ok(body.replace(ATTACHMENTS, &section(&files, &results)))
}

fn upload_all(ctx: &Ctx, repo: &str, files: &[(PathBuf, String)]) -> Vec<Result<String, String>> {
    let mut total = 0;
    let mut ready = false;
    let stamp = crate::procs::now_ms();
    files
        .iter()
        .enumerate()
        .map(|(i, (path, name))| {
            let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if len > MAX_FILE {
                return Err(format!("larger than {} MB", MAX_FILE >> 20));
            }
            total += len;
            if total > MAX_TOTAL {
                return Err(format!(
                    "attachments total more than {} MB",
                    MAX_TOTAL >> 20
                ));
            }
            if !ready {
                ensure_branch(repo)?;
                ready = true;
            }
            let dest = format!("attachments/{stamp}-{i}-{}", safe_name(name));
            put(ctx, repo, path, &dest)?;
            Ok(format!(
                "https://github.com/{repo}/blob/{BRANCH}/{dest}?raw=true"
            ))
        })
        .collect()
}

fn failure(err: &str, fallback: &str) -> String {
    let first = err.lines().map(str::trim).find(|l| !l.is_empty());
    crate::dlog::redact(first.unwrap_or(fallback))
}

fn api(args: &[&str]) -> Result<String, String> {
    let a: Vec<String> = std::iter::once("api")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    match gh_within(&a, UPLOAD_TIMEOUT) {
        Ok((true, out, _)) => Ok(out),
        Ok((false, _, err)) => Err(failure(&err, "gh api failed")),
        Err((_, e)) => Err(e),
    }
}

fn ensure_branch(repo: &str) -> Result<(), String> {
    if api(&[
        &format!("repos/{repo}/git/ref/heads/{BRANCH}"),
        "--jq",
        ".ref",
    ])
    .is_ok()
    {
        return Ok(());
    }
    let default = api(&[&format!("repos/{repo}"), "--jq", ".default_branch"])?;
    let sha = api(&[
        &format!("repos/{repo}/git/ref/heads/{}", default.trim()),
        "--jq",
        ".object.sha",
    ])?;
    api(&[
        "-X",
        "POST",
        &format!("repos/{repo}/git/refs"),
        "-f",
        &format!("ref=refs/heads/{BRANCH}"),
        "-f",
        &format!("sha={}", sha.trim()),
    ])
    .map(|_| ())
}

fn put(ctx: &Ctx, repo: &str, file: &Path, dest: &str) -> Result<(), String> {
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let req = json!({
        "message": "Add feedback attachment",
        "branch": BRANCH,
        "content": base64_encode(&bytes),
    });
    let d = dir(&ctx.state_dir);
    crate::mkdir_private(&d).map_err(|e| e.to_string())?;
    let tmp = d.join(format!(
        "upload-{}-{}.json",
        std::process::id(),
        crate::procs::now_ms()
    ));
    std::fs::write(&tmp, req.to_string()).map_err(|e| e.to_string())?;
    let r = api(&[
        "-X",
        "PUT",
        &format!("repos/{repo}/contents/{dest}"),
        "--input",
        &tmp.to_string_lossy(),
    ]);
    let _ = std::fs::remove_file(&tmp);
    r.map(|_| ())
}

pub fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "file".into()
    } else {
        s
    }
}

pub fn base64_encode(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn section(files: &[(PathBuf, String)], results: &[Result<String, String>]) -> String {
    let home = crate::home();
    files
        .iter()
        .zip(results)
        .map(|((path, name), r)| match r {
            Ok(url) if crate::uploads::ext(name).is_some_and(|e| IMAGES.contains(&e.as_str())) => {
                format!("![{name}]({url})")
            }
            Ok(url) => format!("- [{name}]({url})"),
            Err(why) => format!(
                "- `{}` (not uploaded: {why})",
                crate::uploads::short(path, &home)
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn direct(ctx: &Ctx, b: &Value) -> Res<Value> {
    let repo = text(b, "repo");
    if !valid_repo(repo) {
        return Err(bad("repo must look like owner/name"));
    }
    let title = text(b, "title");
    if title.is_empty() || title.len() > 256 || title.contains('\n') {
        return Err(bad("title must be one line of 1 to 256 characters"));
    }
    let body = checked_body(b)?;
    let labels = labels(b);
    let body = with_attachments(ctx, b, repo, body)?;
    let report = write_report(&ctx.state_dir, &body, SystemTime::now())?;
    let (mut ok, mut out, mut err) = gh(&args(repo, title, &report, &labels))?;
    let mut labelled = !labels.is_empty();
    if !ok && labelled && label_problem(&err) {
        (ok, out, err) = gh(&args(repo, title, &report, &[]))?;
        labelled = false;
    }
    let _ = std::fs::remove_file(&report);
    if !ok {
        let first = err
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("gh issue create failed");
        return Err((502, crate::dlog::redact(first)));
    }
    let url = issue_url(&out).ok_or((502, "gh did not print an issue URL".to_string()))?;
    Ok(json!({ "url": url, "labelled": labelled }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "sb-feedback-{tag}-{}-{}",
            std::process::id(),
            crate::procs::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn repo_slugs() {
        assert!(valid_repo("basilelt/overseer"));
        assert!(valid_repo("a_b/c.d"));
        for r in [
            "", "a", "a/b/c", "-a/b", "a/.b", "a/b c", "a/b;rm", "/b", "a/",
        ] {
            assert!(!valid_repo(r), "{r}");
        }
    }

    #[test]
    fn finds_the_issue_url() {
        let out = "Creating issue in o/r\n\nhttps://github.com/o/r/issues/42\n";
        assert_eq!(
            issue_url(out).as_deref(),
            Some("https://github.com/o/r/issues/42")
        );
        assert_eq!(issue_url("https://github.com/o/r/pull/3"), None);
        assert_eq!(issue_url("https://github.com/o/r/issues/abc"), None);
        assert_eq!(issue_url("nothing here"), None);
    }

    #[test]
    fn builds_gh_arguments() {
        let a = args(
            "o/r",
            "Bug: --help me",
            Path::new("/s/report.md"),
            &["bug".into(), "feedback".into()],
        );
        assert_eq!(
            a,
            [
                "issue",
                "create",
                "--repo=o/r",
                "--title=Bug: --help me",
                "--body-file=/s/report.md",
                "--label=bug",
                "--label=feedback"
            ]
        );
        assert!(!args("o/r", "t", Path::new("/f"), &[])
            .iter()
            .any(|x| x.starts_with("--label")));
    }

    #[test]
    fn keeps_only_sane_labels() {
        let b = json!({"labels": ["bug", " feedback ", "", "-x", "a,b", 3]});
        assert_eq!(labels(&b), ["bug", "feedback"]);
        assert!(labels(&json!({})).is_empty());
    }

    #[test]
    fn retries_without_labels_only_on_label_errors() {
        assert!(label_problem("could not add label: 'feedback' not found"));
        assert!(!label_problem("HTTP 401: Bad credentials"));
    }

    #[test]
    fn writes_private_reports_and_prunes_old_ones() {
        let d = tmp("report");
        let now = SystemTime::now();
        let p = write_report(&d, "hello", now).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let q = write_report(&d, "again", now).unwrap();
        assert_ne!(p, q);
        write_report(&d, "later", now + KEEP + Duration::from_secs(60)).unwrap();
        assert!(!p.exists() && !q.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn falls_back_to_a_stable_workdir() {
        let d = tmp("work");
        let here = std::env::temp_dir();
        assert_eq!(
            workdir(&d, &here.to_string_lossy()).unwrap(),
            here.to_string_lossy()
        );
        let w = workdir(&d, "/no/such/dir").unwrap();
        assert!(w.ends_with("feedback/work") && Path::new(&w).is_dir());
        assert_eq!(workdir(&d, "").unwrap(), w);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn passes_the_prompt_as_the_initial_argument() {
        for id in POSITIONAL {
            let a = crate::agents::get(id).unwrap();
            assert!(a.prompt.is_none(), "{id} has a prompt flag now");
        }
        let cmd = vec!["claude".to_string(), "--model".into(), "opus".into()];
        assert_eq!(
            with_prompt(&cmd, "<prompt>"),
            ["claude", "--model", "opus", "<prompt>"]
        );
    }

    #[test]
    fn fills_the_report_path() {
        assert_eq!(
            fill("Read @REPORT@ first; keep @REPORT@.", Path::new("/s/r.md")),
            "Read /s/r.md first; keep /s/r.md."
        );
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn sanitizes_attachment_names() {
        assert_eq!(safe_name("my shot (1).png"), "my_shot__1_.png");
        assert_eq!(safe_name(""), "file");
    }

    #[test]
    fn renders_links_for_uploads_and_paths_for_failures() {
        let files = vec![
            (PathBuf::from("/u/a1.png"), "shot.png".to_string()),
            (PathBuf::from("/u/b2.mov"), "clip.mov".to_string()),
            (PathBuf::from("/u/c3.txt"), "log.txt".to_string()),
        ];
        let out = section(
            &files,
            &[
                Ok("https://github.com/o/r/blob/x/a.png?raw=true".into()),
                Ok("https://github.com/o/r/blob/x/b.mov?raw=true".into()),
                Err("HTTP 403".into()),
            ],
        );
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "![shot.png](https://github.com/o/r/blob/x/a.png?raw=true)"
        );
        assert_eq!(
            lines[1],
            "- [clip.mov](https://github.com/o/r/blob/x/b.mov?raw=true)"
        );
        assert!(lines[2].contains("/u/c3.txt") && lines[2].ends_with("(not uploaded: HTTP 403)"));
    }

    #[test]
    fn rejects_attachments_outside_the_uploads_dir() {
        let probe = crate::procs::System;
        let ctx = Ctx::from_env(&probe);
        let b = json!({"attachments": [{"path": "/etc/passwd"}]});
        assert_eq!(with_attachments(&ctx, &b, "o/r", "x").unwrap_err().0, 400);
        let none = with_attachments(&ctx, &json!({}), "o/r", "a @ATTACHMENTS@ b").unwrap();
        assert_eq!(none, "a (none) b");
    }

    #[test]
    fn rejects_bad_requests() {
        let probe = crate::procs::System;
        let ctx = Ctx::from_env(&probe);
        let post = |b: Value, path: &[&str]| routes(&ctx, "POST", path, &b).unwrap();
        let direct = ["v1", "feedback", "direct"];
        assert_eq!(
            post(json!({"repo": "x", "title": "t", "body": "b"}), &direct).0,
            400
        );
        assert_eq!(
            post(json!({"repo": "o/r", "title": "", "body": "b"}), &direct).0,
            400
        );
        assert_eq!(
            post(json!({"repo": "o/r", "title": "t", "body": ""}), &direct).0,
            400
        );
        let start = ["v1", "feedback", "start"];
        assert_eq!(
            post(
                json!({"body": "b", "prompt": "no marker", "agent": "claude"}),
                &start
            )
            .0,
            400
        );
        assert_eq!(
            post(
                json!({"body": "b", "prompt": "@REPORT@", "agent": "nope"}),
                &start
            )
            .0,
            400
        );
        assert_eq!(
            post(
                json!({"body": "b", "prompt": "@REPORT@", "agent": "shell"}),
                &start
            )
            .0,
            400
        );
        assert!(routes(&ctx, "GET", &start, &json!({})).is_none());
    }
}
