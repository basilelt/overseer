use overseer::daemon::Request;
use overseer::procs::{Probe, ProcInfo};
use overseer::registry::Registry;
use overseer::{organize, review, Config, Ctx};
use serde_json::json;
use std::path::Path;

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

fn dump(base: &Path, open: impl Fn(&Ctx)) -> serde_json::Value {
    let _ = std::fs::remove_dir_all(base);
    let probe = Nobody;
    let ctx = ctx(base, &probe);
    std::fs::create_dir_all(&ctx.state_dir).unwrap();
    open(&ctx);
    let reg = Registry::open(&ctx.state_dir).unwrap();
    let mut stmt = reg
        .conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    serde_json::Value::Object(rows.into_iter().map(|(n, s)| (n, json!(s))).collect())
}

fn paths() -> serde_json::Value {
    let base =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("schema-{}", std::process::id()));
    let registry = dump(&base.join("registry"), |c| {
        drop(Registry::open(&c.state_dir).unwrap());
    });
    let review = dump(&base.join("review"), |c| {
        drop(review::open(c).unwrap());
    });
    let organize = dump(&base.join("organize"), |c| {
        let r = Request {
            method: "GET".into(),
            path: "/v1/organize/proposals".into(),
            query: vec![],
            headers: vec![],
            body: vec![],
        };
        let (status, _) =
            organize::routes(c, &r, &["v1", "organize", "proposals"], &json!({})).unwrap();
        assert_eq!(status, 200);
    });
    let _ = std::fs::remove_dir_all(&base);
    json!({"registry": registry, "review": review, "organize": organize})
}

#[test]
fn every_open_path_creates_the_recorded_schema() {
    let got = paths();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/schema_paths.json");
    if std::env::var_os("OVERSEER_RECORD_SCHEMA").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(got, want);
}
