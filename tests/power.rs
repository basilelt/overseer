mod common;

use common::{wait, App};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const LID: &str = "/v1/power/lid";

fn inhibitors(log: &Path) -> Vec<(u32, String)> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(pid, args)| (pid.parse().unwrap(), args.to_string()))
        .collect()
}

fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
        !s.rsplit(')')
            .next()
            .unwrap_or("")
            .trim_start()
            .starts_with('Z')
    })
}

fn app(tag: &str) -> (App, PathBuf) {
    let log = std::env::temp_dir().join(format!("sb-inhibit-{tag}-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_file(log.with_extension("log.fail"));
    let app = App::with_env(tag, json!({}), &[("SB_INHIBIT_LOG", log.to_str().unwrap())]);
    (app, log)
}

#[test]
fn enabling_holds_a_lid_inhibitor_that_disabling_releases() {
    let (app, log) = app("power-on-off");
    let v = app.ok("GET", LID, Value::Null);
    assert_eq!(
        v,
        json!({"enabled": false, "active": false, "supported": true, "error": null})
    );

    let v = app.ok("PUT", LID, json!({"enabled": true}));
    assert_eq!(
        v,
        json!({"enabled": true, "active": true, "supported": true, "error": null})
    );
    let held = inhibitors(&log);
    assert_eq!(held.len(), 1);
    assert_eq!(
        held[0].1,
        "--what=handle-lid-switch --no-ask-password --mode=block --who=Overseer --why=Keep agents running with the lid closed cat"
    );
    assert!(alive(held[0].0));

    app.ok("PUT", LID, json!({"enabled": true}));
    assert_eq!(inhibitors(&log).len(), 1);

    let v = app.ok("PUT", LID, json!({"enabled": false}));
    assert_eq!(v["enabled"], false);
    assert_eq!(v["active"], false);
    wait(|| !alive(held[0].0));
}

#[test]
fn the_inhibitor_dies_with_a_killed_daemon_and_comes_back_on_restart() {
    let (mut app, log) = app("power-crash");
    app.ok("PUT", LID, json!({"enabled": true}));
    let first = inhibitors(&log)[0].0;
    app.crash_and_restart();
    wait(|| !alive(first));
    let v = app.ok("GET", LID, Value::Null);
    assert_eq!(v["enabled"], true);
    assert_eq!(v["active"], true);
    let held = inhibitors(&log);
    assert_eq!(held.len(), 2);
    assert!(alive(held[1].0));
}

#[test]
fn a_refused_inhibitor_keeps_the_option_off_and_shows_why() {
    let (app, log) = app("power-refused");
    std::fs::write(log.with_extension("log.fail"), "").unwrap();
    let (code, v) = app.req("PUT", LID, json!({"enabled": true}));
    assert_eq!(code, 500);
    assert_eq!(v["error"], "Failed to inhibit: Access denied");
    let v = app.ok("GET", LID, Value::Null);
    assert_eq!(v["enabled"], false);
    assert_eq!(v["active"], false);
    assert_eq!(v["error"], "Failed to inhibit: Access denied");

    std::fs::remove_file(log.with_extension("log.fail")).unwrap();
    let v = app.ok("PUT", LID, json!({"enabled": true}));
    assert_eq!(v["error"], Value::Null);
    assert_eq!(v["active"], true);
}

#[test]
fn a_refused_change_is_forgotten_after_a_restart() {
    let (mut app, log) = app("power-refused-restart");
    std::fs::write(log.with_extension("log.fail"), "").unwrap();
    let (code, _) = app.req("PUT", LID, json!({"enabled": true}));
    assert_eq!(code, 500);
    assert_eq!(
        app.ok("GET", LID, Value::Null)["error"],
        "Failed to inhibit: Access denied"
    );
    app.crash_and_restart();
    assert_eq!(
        app.ok("GET", LID, Value::Null),
        json!({"enabled": false, "active": false, "supported": true, "error": null})
    );
}

#[test]
fn a_non_boolean_body_is_rejected() {
    let (app, _log) = app("power-bad");
    let (code, v) = app.req("PUT", LID, json!({"enabled": "yes"}));
    assert_eq!(code, 400);
    assert_eq!(v["error"], "enabled must be a boolean");
}
