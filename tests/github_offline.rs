mod common;

use common::{Fx, Reap};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

const OK: &str = r#"{"data":{"viewer":{"login":"me"},"search":{"nodes":[{"number":7,"title":"Add x","url":"https://github.com/acme/api/pull/7","repository":{"nameWithOwner":"acme/api"},"author":{"login":"bob"},"headRefName":"x","baseRefName":"main","updatedAt":"2026-01-01T00:00:00Z","isDraft":false,"additions":1,"deletions":2,"changedFiles":1,"reviewDecision":null,"reviews":{"nodes":[]}}]}}}"#;

const GH: &str = r#"#!/bin/sh
d=$(dirname "$0")
mode=$(cat "$d/mode")
case "$1 $2" in
  "auth status") [ "$mode" = ok ] && exit 0; echo 'The token in /x/hosts.yml is invalid' >&2; exit 1 ;;
  "auth token") [ "$mode" = loggedout ] && { echo 'no oauth token found for github.com' >&2; exit 1; }; echo gho_fake; exit 0 ;;
esac
case "$mode" in
  ok) cat "$d/reviews.json" ;;
  *) echo 'Post "https://api.github.com/graphql": proxyconnect tcp: dial tcp 127.0.0.1:9: connect: connection refused' >&2; exit 1 ;;
esac
"#;

fn daemon(fx: &Fx, bin: &std::path::Path) -> Reap {
    let mut c = Command::new(env!("CARGO_BIN_EXE_overseer"));
    fx.env(&mut c);
    c.env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .args(["daemon", "--interval", "1"]);
    let r = Reap::spawn(c.stderr(Stdio::null()));
    common::wait(|| std::os::unix::net::UnixStream::connect(&fx.sock).is_ok());
    r
}

fn get(fx: &Fx) -> (u16, Value) {
    let (code, body) = fx.req("GET", "/v1/github/review-requests?refresh=1", "");
    (code, serde_json::from_str(&body).unwrap_or(Value::Null))
}

#[test]
fn review_requests_tell_offline_from_logged_out_through_the_daemon() {
    let fx = Fx::new("ghoffline", 0);
    let bin = fx.base.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh.src"), GH).unwrap();
    assert!(Command::new("cp")
        .arg(bin.join("gh.src"))
        .arg(bin.join("gh"))
        .status()
        .unwrap()
        .success());
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(bin.join("reviews.json"), OK).unwrap();
    let _d = daemon(&fx, &bin);

    std::fs::write(bin.join("mode"), "offline").unwrap();
    let (code, v) = get(&fx);
    assert_eq!(code, 502, "{v}");
    assert_eq!(v["code"], "failed", "{v}");
    let msg = v["error"].as_str().unwrap();
    assert!(msg.starts_with("GitHub could not be reached"), "{msg}");
    assert!(msg.contains("connection refused"), "{msg}");
    assert!(!msg.contains("not logged in"), "{msg}");

    std::fs::write(bin.join("mode"), "loggedout").unwrap();
    let (code, v) = get(&fx);
    assert_eq!(code, 424, "{v}");
    assert_eq!(v["code"], "auth", "{v}");
    assert_eq!(v["error"], "gh is not logged in on this host");

    std::fs::write(bin.join("mode"), "ok").unwrap();
    let (code, v) = get(&fx);
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["login"], "me");
    assert_eq!(v["prs"][0]["repo"], "acme/api");
}
