use overseer::agents::{self, Launch};
use overseer::procs::{Probe, ProcInfo};
use overseer::registry::Registry;
use overseer::{lifecycle, review, tasks, upgrade, worktrees, Config, Ctx};
use serde_json::{json, Map, Value};
use std::path::Path;

const NAMES: &[&str] = &[
    "feat/AB-12 Add login",
    "Fix: crash on résumé!",
    "release/v1.2.3",
    "  --weird__name..  ",
    "UPPER_case.and-dots",
    "a/b/c/d",
    "",
    "a-very-long-branch-name-that-goes-well-beyond-forty-characters-in-total",
];

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
        vibe_dir: base.join("vibe"),
        state_dir: base.join("state"),
        snapshot: base.join("snapshot.json"),
        now_ms: 0,
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
            worktree_stale_days: 0,
            session_tasks_days: 0,
            discover_roots: vec![],
        },
    }
}

fn table(f: impl Fn(&str) -> String) -> Value {
    Value::Object(NAMES.iter().map(|n| (n.to_string(), json!(f(n)))).collect())
}

fn keys(v: Value) -> Value {
    json!(v.as_object().unwrap().keys().collect::<Vec<_>>())
}

fn launches() -> Value {
    let extra = vec!["--x".to_string()];
    let mut out = Map::new();
    for a in agents::AGENTS {
        let launch = |auto_approve| {
            let argv = a.launch(&Launch {
                model: "m1",
                effort: "max",
                session: Some("sid-1"),
                prompt: Some("do it"),
                extra: &extra,
                auto_approve,
            });
            argv[1..].to_vec()
        };
        let resume = overseer::resume_argv(a.id, "sid-1", &extra);
        out.insert(
            a.id.to_string(),
            json!({
                "launch": launch(false),
                "launchAutoApprove": launch(true),
                "resume": resume[1..],
                "resumeFlags": overseer::resume_flags(a.id, &extra),
            }),
        );
    }
    Value::Object(out)
}

fn schema(base: &Path) -> Value {
    let probe = Nobody;
    let ctx = ctx(base, &probe);
    std::fs::create_dir_all(&ctx.state_dir).unwrap();
    drop(Registry::open(&ctx.state_dir).unwrap());
    let reg = review::open(&ctx).unwrap();
    let mut stmt = reg
        .conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Object(rows.into_iter().map(|(n, s)| (n, json!(s))).collect())
}

fn formats(base: &Path) -> Value {
    json!({
        "taskSlug": table(tasks::slug),
        "worktreeDirName": table(worktrees::dir_name),
        "fnv": table(worktrees::fnv),
        "agents": launches(),
        "savedTermKeys": keys(serde_json::to_value(upgrade::SavedTerm::default()).unwrap()),
        "lifecycleSavedKeys": keys(serde_json::to_value(lifecycle::Saved::default()).unwrap()),
        "schema": schema(base),
    })
}

#[test]
fn names_argv_records_and_schema_match_the_recorded_formats() {
    let base =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("formats-{}", std::process::id()));
    let got = formats(&base);
    let _ = std::fs::remove_dir_all(&base);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/formats.json");
    if std::env::var_os("OVERSEER_RECORD_FORMATS").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        got, want,
        "persisted names, argv or schema changed: old state and running sessions depend on them"
    );
}
