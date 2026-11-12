use overseer::install;
use serde_json::{json, Map, Value};
use std::path::Path;

const GOLDEN: &str = "tests/golden/hook_files.json";

fn render(home: &Path) -> Value {
    std::env::set_var("HOME", home);
    std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    std::env::set_var("CLAUDE_CONFIG_DIR", home.join(".claude"));
    std::env::set_var("CODEX_HOME", home.join(".codex"));
    let bin = Path::new("/opt/o v/overseer");
    let state = Path::new("/var/st ate");
    let mut out = Map::new();
    for t in install::targets() {
        let p = install::plan(&t, false, bin, state).unwrap();
        let rel = t.path.strip_prefix(home).unwrap().to_string_lossy();
        out.insert(
            t.agent.to_string(),
            json!({ "path": rel, "text": p.after_text, "ours": p.ours }),
        );
    }
    Value::Object(out)
}

#[test]
fn every_agent_target_generates_the_same_bytes() {
    let home =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hook-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let got = render(&home);
    let _ = std::fs::remove_dir_all(&home);
    if std::env::var_os("RECORD_GOLDEN").is_some() {
        std::fs::write(GOLDEN, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: Value = serde_json::from_slice(&std::fs::read(GOLDEN).unwrap()).unwrap();
    assert_eq!(got, want);
}
