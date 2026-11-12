use overseer::procs::{Probe, ProcInfo};
use overseer::{doctor, Config, Ctx};
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

fn config(base: &Path) -> Config {
    Config {
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
    }
}

fn lines(base: &Path, home: &Path, probe: &Nobody) -> Vec<String> {
    let ctx = Ctx {
        claude_dir: home.join(".claude"),
        codex_dir: home.join(".codex"),
        gemini_dir: home.join(".gemini"),
        vibe_dir: home.join(".vibe"),
        state_dir: base.join("state"),
        snapshot: base.join("snapshot.json"),
        now_ms: 0,
        probe,
        tmux: None,
        config: config(base),
    };
    let base_s = base.to_string_lossy().into_owned();
    doctor::run(&ctx)
        .into_iter()
        .map(|i| {
            let detail = if i.name == "api reachable" && i.level == "fail" {
                "<error>".to_string()
            } else if i.name == "lines" {
                i.detail.split("; memory").next().unwrap().to_string()
            } else {
                i.detail.replace(&base_s, "$BASE")
            };
            format!("{}|{}|{}", i.level, i.name, detail)
        })
        .collect()
}

#[test]
fn doctor_reports_the_same_lines_for_a_fixture_home() {
    let base =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let home = base.join("home");
    std::fs::create_dir_all(base.join("state")).unwrap();
    for d in [".claude/hooks", ".codex", ".local/bin", "bin"] {
        std::fs::create_dir_all(home.join(d)).unwrap();
    }
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(base.join("state"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
    }
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", base.join("bin"));
    std::env::remove_var("OVERSEER_STATE_DIR");
    std::fs::write(
        home.join(".claude/settings.json"),
        r##"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x #overseer-hook"}]}]}}"##,
    )
    .unwrap();
    std::fs::write(home.join(".claude/hooks/tmux-claude-id.sh"), "").unwrap();
    std::fs::write(home.join(".local/bin/tmux-snapshot"), "sb_held").unwrap();
    std::fs::write(home.join(".local/bin/tmux-restore"), "").unwrap();
    std::fs::write(
        base.join("state/sync.json"),
        r#"{"api":"http://127.0.0.1:1"}"#,
    )
    .unwrap();
    let probe = Nobody;
    let got = lines(&base, &home, &probe).join("\n");
    let _ = std::fs::remove_dir_all(&base);
    let want = std::fs::read_to_string("tests/golden/doctor_output.txt").unwrap_or_default();
    if std::env::var_os("RECORD_GOLDEN").is_some() {
        std::fs::write("tests/golden/doctor_output.txt", &got).unwrap();
        return;
    }
    assert_eq!(got, want);
}
