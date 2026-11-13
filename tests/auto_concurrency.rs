mod common;

use common::{wait, App};
use serde_json::{json, Value};

const PATH: &str = "/v1/tasks/auto-concurrency";

fn started(app: &App) -> usize {
    app.ok("GET", "/v1/tasks", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["status"] == "in_progress")
        .count()
}

fn auto_tasks(app: &App, n: usize) {
    let wt = app.fx.wt.to_string_lossy().into_owned();
    for i in 0..n {
        app.ok(
            "POST",
            "/v1/tasks",
            json!({"title": format!("job {i}"), "project": wt, "status": "auto"}),
        );
    }
}

#[test]
fn the_limit_defaults_to_the_config_and_roundtrips() {
    let app = App::new("conc-default", json!({"autoConcurrency": 4}));
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"value": 4}));
    assert_eq!(
        app.ok("PUT", PATH, json!({"value": 7})),
        json!({"value": 7})
    );
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"value": 7}));
}

#[test]
fn a_value_outside_one_to_sixteen_is_refused() {
    let app = App::new("conc-invalid", json!({"autoConcurrency": 2}));
    for v in [
        json!(0),
        json!(17),
        json!(-1),
        json!(2.5),
        json!("3"),
        Value::Null,
    ] {
        let (code, r) = app.req("PUT", PATH, json!({ "value": v }));
        assert_eq!(
            (code, r["error"].clone()),
            (400, json!("value must be an integer from 1 to 16")),
            "{v}"
        );
    }
    let (code, r) = app.req("PUT", PATH, json!({}));
    assert_eq!(
        (code, r["error"].clone()),
        (400, json!("value must be an integer from 1 to 16"))
    );
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"value": 2}));
}

#[test]
fn the_limit_is_enforced_live_and_survives_a_crash() {
    let mut app = App::new("conc-live", json!({"autoConcurrency": 0}));
    app.ok("PUT", PATH, json!({"value": 1}));
    auto_tasks(&app, 3);
    wait(|| started(&app) == 1);
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!(started(&app), 1);
    app.crash_and_restart();
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"value": 1}));
    app.ok("PUT", PATH, json!({"value": 3}));
    wait(|| started(&app) == 3);
}
