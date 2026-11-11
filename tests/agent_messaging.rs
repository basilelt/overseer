mod common;

use common::{git, App};
use serde_json::{json, Value};

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

fn second_repo(app: &App) -> String {
    let dir = app.fx.base.join("other");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "dev@example.com"]);
    git(&dir, &["config", "user.name", "Dev"]);
    std::fs::write(dir.join("b.txt"), "two\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "init"]);
    dir.canonicalize().unwrap().to_string_lossy().into_owned()
}

fn task(app: &App, title: &str, project: &str, parent: Option<&str>) -> String {
    let mut b = json!({"title": title, "project": project});
    if let Some(p) = parent {
        b["parentId"] = json!(p);
    }
    id(&app.ok("POST", "/v1/tasks", b))
}

fn send(app: &App, to: &str, from: Option<&str>) -> (u16, Value) {
    app.req(
        "POST",
        &format!("/v1/tasks/{to}/messages"),
        json!({"body": "hello", "from": from}),
    )
}

#[test]
fn the_switch_is_on_by_default_and_roundtrips() {
    let app = App::new("msg-switch", json!({"autoConcurrency": 0}));
    let path = "/v1/tasks/agent-messaging";
    assert_eq!(app.ok("GET", path, Value::Null), json!({"enabled": true}));
    assert_eq!(
        app.ok("PUT", path, json!({"enabled": false})),
        json!({"enabled": false})
    );
    assert_eq!(app.ok("GET", path, Value::Null), json!({"enabled": false}));
    let (code, v) = app.req("PUT", path, json!({"enabled": "no"}));
    assert_eq!(
        (code, v["error"].clone()),
        (400, json!("enabled must be a boolean"))
    );
    app.ok("PUT", path, json!({"enabled": true}));
    assert_eq!(app.ok("GET", path, Value::Null), json!({"enabled": true}));
}

#[test]
fn agents_message_their_own_project_but_not_another() {
    let app = App::new("msg-project", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let other = second_repo(&app);
    let (a, b) = (task(&app, "A", &wt, None), task(&app, "B", &wt, None));
    let far = task(&app, "Far", &other, None);
    let (code, v) = send(&app, &b, Some(&a));
    assert_eq!(code, 201, "{v}");
    assert_eq!(v["from"], a.as_str());
    let (code, v) = send(&app, &far, Some(&a));
    assert_eq!(code, 403);
    assert!(
        v["error"].as_str().unwrap().contains("another project"),
        "{v}"
    );
    let (code, _) = send(&app, &a, Some(&far));
    assert_eq!(code, 403);
    assert_eq!(send(&app, &far, None).0, 201);
    let msgs = app.ok("GET", &format!("/v1/tasks/{far}/messages"), Value::Null);
    assert_eq!(msgs.as_array().unwrap().len(), 1);
}

#[test]
fn switching_off_blocks_agents_but_not_the_user_or_a_task_and_its_subtask() {
    let app = App::new("msg-off", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let (a, b) = (task(&app, "A", &wt, None), task(&app, "B", &wt, None));
    let sub = task(&app, "Sub", &wt, Some(&a));
    app.ok(
        "PUT",
        "/v1/tasks/agent-messaging",
        json!({"enabled": false}),
    );
    let (code, v) = send(&app, &b, Some(&a));
    assert_eq!(code, 403);
    assert!(v["error"].as_str().unwrap().contains("turned off"), "{v}");
    assert_eq!(send(&app, &b, None).0, 201);
    assert_eq!(send(&app, &sub, Some(&a)).0, 201);
    assert_eq!(send(&app, &a, Some(&sub)).0, 201);
    app.ok("PUT", "/v1/tasks/agent-messaging", json!({"enabled": true}));
    assert_eq!(send(&app, &b, Some(&a)).0, 201);
}
