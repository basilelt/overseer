mod common;

use common::{enc_uri_component, git, Fx, Reap};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn write(dir: &Path, f: &str, text: &str) {
    let p = dir.join(f);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
}

fn repo(base: &Path, name: &str) -> PathBuf {
    let d = base.join(name);
    std::fs::create_dir_all(&d).unwrap();
    git(&d, &["init", "-q", "-b", "main"]);
    git(&d, &["config", "user.email", "dev@example.com"]);
    git(&d, &["config", "user.name", "Dev"]);
    d.canonicalize().unwrap()
}

struct M {
    fx: Fx,
    _d: Reap,
    api: PathBuf,
}

fn setup(tag: &str) -> M {
    let fx = Fx::new(tag, 0);
    let lib = repo(&fx.base, "lib-src");
    write(
        &lib,
        "src/arch.rs",
        "pub fn parse_arch(s: &str) -> u8 {\n    0\n}\n",
    );
    commit_all(&lib, "lib");
    write(
        &fx.wt,
        "src/build.rs",
        "pub fn push_manifest() -> u8 {\n    1\n}\n\npub fn tag_of() -> u8 {\n    2\n}\n",
    );
    write(&fx.wt, "tests/build.rs", "fn smoke() {}\n");
    commit_all(&fx.wt, "base");
    git(
        &fx.wt,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            lib.to_str().unwrap(),
            "vendor/lib",
        ],
    );
    commit_all(&fx.wt, "submodule");
    write(&fx.wt, "src/build.rs", "pub fn push_manifest() -> u8 {\n    if tag_of() > 1 {\n        return 3;\n    }\n    1\n}\n\npub fn tag_of() -> u8 {\n    if true { 2 } else { 4 }\n}\n");
    write(
        &fx.wt,
        "src/new.rs",
        "pub fn fresh_helper() -> u8 {\n    5\n}\n",
    );
    let sub = fx.wt.join("vendor/lib");
    write(
        &sub,
        "src/arch.rs",
        "pub fn parse_arch(s: &str) -> u8 {\n    if s == \"arm64\" { 1 } else { 0 }\n}\n",
    );
    let api = repo(&fx.base, "api");
    write(
        &api,
        "src/push.rs",
        "pub fn push_manifest() -> u8 {\n    0\n}\n",
    );
    commit_all(&api, "api");
    write(
        &api,
        "src/push.rs",
        "pub fn push_manifest() -> u8 {\n    let n = 7;\n    n\n}\n",
    );
    let d = fx.daemon(false);
    M { fx, _d: d, api }
}

impl M {
    fn get(&self, query: &str) -> (u16, Value) {
        let (c, b) = self.fx.req("GET", &format!("/v1/review/map?{query}"), "");
        (c, serde_json::from_str(&b).unwrap_or(Value::Null))
    }
}

fn region<'a>(v: &'a Value, name: &str) -> &'a Value {
    v["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no region {name} in {v}"))
}

fn changed<'a>(r: &'a Value, path: &str) -> &'a Value {
    r["changed"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == path)
        .unwrap_or_else(|| panic!("{path} not changed in {r}"))
}

#[test]
fn maps_every_worktree_of_the_review_with_its_submodule_and_links() {
    let m = setup("rmap");
    let wt = m.fx.wt.to_string_lossy().into_owned();
    let api = m.api.to_string_lossy().into_owned();
    let (code, v) = m.get(&format!(
        "worktree={}&with={}",
        enc_uri_component(&wt),
        enc_uri_component(&api)
    ));
    assert_eq!(code, 200, "{v}");
    assert!(v.get("error").is_none(), "{v}");
    let regions = v["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 3, "{v}");
    let main = region(&v, "repo");
    assert_eq!(main["worktree"], wt.as_str());
    assert!(main["parent"].is_null());
    let lib = region(&v, "lib");
    assert_eq!(lib["parent"], 0);
    let other = region(&v, "api");
    assert!(other["parent"].is_null());

    let build = changed(main, "src/build.rs");
    assert!(build["logic"].as_u64().unwrap() > 0, "{build}");
    assert!(build["size"].as_u64().unwrap() > 0);
    assert_eq!(build["test"], false);
    assert_eq!(build["viewed"], false);
    assert_eq!(changed(main, "src/new.rs")["status"], "untracked");
    assert!(changed(lib, "src/arch.rs")["add"].as_u64().unwrap() > 0);
    assert!(changed(other, "src/push.rs")["add"].as_u64().unwrap() > 0);

    let tree: Vec<&str> = main["tree"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e[0].as_str().unwrap())
        .collect();
    assert!(tree.contains(&"a.txt"), "{tree:?}");
    assert!(tree.contains(&"tests/build.rs"));
    assert!(!tree.contains(&"src/build.rs"));
    assert!(!tree.contains(&"vendor/lib"));

    let links = v["links"].as_array().unwrap();
    let probable = links
        .iter()
        .find(|l| l["probable"] == true)
        .unwrap_or_else(|| panic!("no cross link in {links:?}"));
    assert_eq!(probable["cross"], true);
    assert_eq!(probable["from"]["path"], "src/push.rs");
    assert_eq!(probable["to"]["path"], "src/build.rs");
}

#[test]
fn rejects_a_bad_extra_worktree_like_the_review() {
    let m = setup("rmap-bad");
    let wt = m.fx.wt.to_string_lossy().into_owned();
    let (code, v) = m.get(&format!(
        "worktree={}&with=relative%2Fdir",
        enc_uri_component(&wt)
    ));
    assert_eq!(code, 400);
    assert_eq!(v["error"], "relative/dir is not an absolute path");
    let gone = m.fx.base.join("gone").to_string_lossy().into_owned();
    let (code, v) = m.get(&format!(
        "worktree={}&with={}",
        enc_uri_component(&wt),
        enc_uri_component(&gone)
    ));
    assert_eq!(code, 410);
    assert_eq!(
        v["error"],
        format!("the worktree folder {gone} no longer exists")
    );
    let (code, v) = m.get("");
    assert_eq!(code, 400, "{v}");
}

#[test]
fn opens_a_submodule_as_its_own_map() {
    let m = setup("rmap-sub");
    let sub = m.fx.wt.join("vendor/lib").to_string_lossy().into_owned();
    let (code, v) = m.get(&format!("worktree={}", enc_uri_component(&sub)));
    assert_eq!(code, 200, "{v}");
    let r = &v["regions"][0];
    assert_eq!(r["name"], "lib");
    assert!(changed(r, "src/arch.rs")["logic"].as_u64().unwrap() > 0);
}
