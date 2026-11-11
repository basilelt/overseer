mod common;

use overseer::daemon::Request;
use overseer::procs::{Probe, ProcInfo};
use overseer::{Config, Ctx};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

struct Nobody;

impl Probe for Nobody {
    fn info(&self, _: u32) -> Option<ProcInfo> {
        None
    }
    fn list(&self) -> Vec<(u32, Vec<String>)> {
        vec![]
    }
    fn cwd(&self, _: u32) -> Option<String> {
        None
    }
}

fn ctx<'a>(base: &Path, probe: &'a Nobody) -> Ctx<'a> {
    Ctx {
        claude_dir: base.join("claude"),
        codex_dir: base.join("codex"),
        gemini_dir: base.join("gemini"),
        state_dir: base.join("state"),
        snapshot: base.join("snapshot.json"),
        now_ms: overseer::procs::now_ms(),
        probe,
        tmux: None,
        config: Config {
            station_id: "test".into(),
            owner: "t@example.com".into(),
            roots: vec![],
            idle_after_ms: 600_000,
            mcp_hints: vec![],
            accept_remote_commands: false,
            ssh: Default::default(),
            remote_command: "overseer".into(),
            repos: Default::default(),
            worktree_dir: base.join("store"),
            start_args: vec![],
            memory_policy: None,
            auto_concurrency: 0,
            auto_organize: false,
            worktree_fetch: false,
            worktree_stale_days: 14,
            session_tasks_days: 7,
            discover_roots: vec![],
        },
    }
}

fn stub(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn call(ctx: &Ctx, method: &str, path: &str, query: &[(&str, &str)], body: Value) -> (u16, Value) {
    let r = Request {
        method: method.into(),
        path: path.into(),
        query: query
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        headers: vec![],
        body: vec![],
    };
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    overseer::task_sources::routes(ctx, &r, &segs, &body).expect("route matched")
}

#[test]
fn a_project_lists_issues_from_its_remote_and_added_sources() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("srcs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (bin, repo) = (base.join("bin"), base.join("repo"));
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
    std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    std::env::set_var("PATH", path);
    for args in [
        &["init", "-q"][..],
        &["remote", "add", "origin", "git@github.com:acme/api.git"],
    ] {
        assert!(Command::new("git")
            .current_dir(&repo)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    stub(
        &bin,
        "gh",
        r#"echo "$@" > "$(dirname "$0")/gh.args"; echo '[{"number":7,"title":"Fix login","body":"b","url":"https://github.com/acme/api/issues/7","labels":[{"name":"bug"}]}]'"#,
    );
    stub(
        &bin,
        "jira",
        r#"echo "$@" > "$(dirname "$0")/jira.args"; printf 'Story\t\tAS-12\tExport CSV\n'"#,
    );
    stub(
        &bin,
        "glab",
        "echo 'glab: 404 Project Not Found' >&2; exit 1",
    );
    let probe = Nobody;
    let c = ctx(&base, &probe);
    let project = repo.canonicalize().unwrap().to_string_lossy().into_owned();
    let encoded = project.replace('/', "%2F");
    let p = [("project", encoded.as_str())];

    let (code, v) = call(&c, "GET", "/v1/projects/sources", &p, Value::Null);
    assert_eq!(code, 200);
    assert_eq!(
        v,
        json!([{"id": "github", "kind": "github", "target": "acme/api", "implicit": true}])
    );

    let (code, jira) = call(
        &c,
        "POST",
        "/v1/projects/sources",
        &[],
        json!({"project": project, "kind": "jira", "target": "project = \"AS\""}),
    );
    assert_eq!(code, 201);
    assert_eq!(
        call(
            &c,
            "POST",
            "/v1/projects/sources",
            &[],
            json!({"project": project, "kind": "gitlab", "target": "grp/gone"})
        )
        .0,
        201
    );
    assert_eq!(
        call(
            &c,
            "POST",
            "/v1/projects/sources",
            &[],
            json!({"project": project, "kind": "gitlab", "target": "--help/x"})
        )
        .0,
        400
    );
    assert_eq!(
        call(
            &c,
            "GET",
            "/v1/projects/sources",
            &[("project", "rel")],
            Value::Null
        )
        .0,
        400
    );

    let (code, v) = call(&c, "GET", "/v1/projects/items", &p, Value::Null);
    assert_eq!(code, 200);
    let keys: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["#7", "AS-12"]);
    assert_eq!(v["items"][1]["ticketKey"], "AS-12");
    assert_eq!(v["items"][1]["title"], "Export CSV");
    assert_eq!(v["items"][0]["taskType"], "fix");
    assert_eq!(v["items"][1]["taskType"], "feature");
    let jira_args = std::fs::read_to_string(bin.join("jira.args")).unwrap();
    assert!(
        jira_args.contains("--columns type,key,summary"),
        "{jira_args}"
    );
    assert_eq!(v["errors"].as_array().unwrap().len(), 1);
    assert!(v["errors"][0]["error"]
        .as_str()
        .unwrap()
        .contains("404 Project Not Found"));
    let args = std::fs::read_to_string(bin.join("gh.args")).unwrap();
    assert!(
        args.starts_with("issue list -R acme/api --state open"),
        "{args}"
    );
    assert!(
        args.contains("--json number,title,body,url,labels"),
        "{args}"
    );

    assert_eq!(
        call(
            &c,
            "DELETE",
            &format!("/v1/projects/sources/{}", jira["id"].as_str().unwrap()),
            &[],
            Value::Null
        )
        .0,
        200
    );
    assert_eq!(
        call(
            &c,
            "DELETE",
            "/v1/projects/sources/github",
            &[],
            Value::Null
        )
        .0,
        404
    );
    assert_eq!(
        call(&c, "GET", "/v1/projects/sources", &p, Value::Null)
            .1
            .as_array()
            .unwrap()
            .len(),
        2
    );
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn the_daemon_serves_sources_for_a_project_path_encoded_like_the_app_sends_it() {
    let fx = common::Fx::new("task-sources", 0);
    std::fs::write(
        fx.base.join("gh/_.json"),
        r#"[{"number":3,"title":"Crash on start","body":"","url":"https://github.com/Acme/board/issues/3"}]"#,
    )
    .unwrap();
    let _d = fx.daemon(false);
    let project: String = fx
        .wt
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();

    let (code, body) = fx.req(
        "GET",
        &format!("/v1/projects/sources?project={project}"),
        "",
    );
    assert_eq!(code, 200, "{body}");
    assert!(body.contains(r#""target":"Acme/board""#), "{body}");

    let (code, body) = fx.req("GET", &format!("/v1/projects/items?project={project}"), "");
    assert_eq!(code, 200, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["items"][0]["key"], "#3", "{body}");
    assert_eq!(v["errors"], json!([]));
}
