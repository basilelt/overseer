use crate::daemon::{q, Request};
use crate::git::Git;
use crate::registry::Registry;
use crate::tasks::{bad, db, main_root, sql};
use crate::util::{ticket, ulid};
use crate::Ctx;
use rusqlite::params;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

type Res<T> = Result<T, (u16, String)>;

const TIMEOUT: Duration = Duration::from_secs(30);
const LIMIT: &str = "100";
pub const KINDS: [&str; 3] = ["github", "gitlab", "jira"];

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub implicit: bool,
}

impl Source {
    fn to_json(&self) -> Value {
        json!({"id": self.id, "kind": self.kind, "target": self.target, "implicit": self.implicit})
    }
}

pub fn routes(ctx: &Ctx, r: &Request, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    let out = match (r.method.as_str(), segs) {
        ("GET", ["v1", "projects", "sources"]) => project(r).and_then(|p| list(ctx, &p)).map(|v| {
            (
                200,
                Value::from(v.iter().map(Source::to_json).collect::<Vec<_>>()),
            )
        }),
        ("POST", ["v1", "projects", "sources"]) => add(ctx, body).map(|s| (201, s.to_json())),
        ("DELETE", ["v1", "projects", "sources", id]) => {
            remove(ctx, id).map(|_| (200, json!({"ok": true})))
        }
        ("GET", ["v1", "projects", "items"]) => {
            project(r).and_then(|p| items(ctx, &p)).map(|v| (200, v))
        }
        _ => return None,
    };
    Some(out.unwrap_or_else(|(c, e)| (c, json!({ "error": e }))))
}

fn project(r: &Request) -> Res<String> {
    q(r, "project")
        .map(crate::board::decode)
        .as_deref()
        .map(str::trim)
        .filter(|p| Path::new(p).is_absolute())
        .map(String::from)
        .ok_or_else(|| bad("project must be an absolute path"))
}

fn remote_source(repo: &str) -> Option<(&'static str, String)> {
    if let Some(r) = repo.strip_prefix("github.com/") {
        return Some(("github", r.into()));
    }
    if let Some(r) = repo.strip_prefix("gitlab.com/") {
        return Some(("gitlab", r.into()));
    }
    let (host, _) = repo.split_once('/')?;
    host.contains("gitlab")
        .then(|| ("gitlab", format!("https://{repo}")))
}

fn implicit(project: &str) -> Option<Source> {
    let (_, repo) = main_root(&mut Git::default(), project)?;
    remote_source(&repo).map(|(kind, target)| Source {
        id: kind.into(),
        kind: kind.into(),
        target,
        implicit: true,
    })
}

fn stored(reg: &Registry, project: &str) -> Res<Vec<Source>> {
    let mut st = sql(reg.conn.prepare(
        "SELECT id, kind, target FROM task_sources WHERE project = ?1 ORDER BY created_ms, rowid",
    ))?;
    let rows = sql(st.query_map([project], |r| {
        Ok(Source {
            id: r.get(0)?,
            kind: r.get(1)?,
            target: r.get(2)?,
            implicit: false,
        })
    }))?;
    sql(rows.collect())
}

pub fn list(ctx: &Ctx, project: &str) -> Res<Vec<Source>> {
    let explicit = stored(&db(ctx)?, project)?;
    let mut v: Vec<Source> = implicit(project)
        .filter(|i| {
            !explicit
                .iter()
                .any(|s| s.kind == i.kind && s.target == i.target)
        })
        .into_iter()
        .collect();
    v.extend(explicit);
    Ok(v)
}

fn valid_path(s: &str, min: usize, max: usize) -> bool {
    let ok = |p: &str| {
        p.starts_with(|c: char| c.is_ascii_alphanumeric())
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    let parts: Vec<&str> = s.split('/').collect();
    (min..=max).contains(&parts.len()) && parts.into_iter().all(ok)
}

fn valid_gitlab(s: &str) -> bool {
    match s.strip_prefix("https://") {
        Some(rest) => valid_path(rest, 3, 21),
        None => valid_path(s, 2, 20),
    }
}

pub fn validate(kind: &str, target: &str) -> Res<()> {
    match kind {
        "github" if valid_path(target, 2, 2) => Ok(()),
        "github" => Err(bad("GitHub source must look like owner/repo")),
        "gitlab" if valid_gitlab(target) => Ok(()),
        "gitlab" => Err(bad(
            "GitLab source must look like group/repo or https://host/group/repo",
        )),
        "jira" if !target.is_empty() && target.len() <= 1000 => Ok(()),
        "jira" => Err(bad("Jira source needs a JQL query")),
        _ => Err(bad(format!("kind must be one of {}", KINDS.join(", ")))),
    }
}

pub fn insert(reg: &Registry, project: &str, kind: &str, target: &str) -> Res<Source> {
    let target = target.trim();
    validate(kind, target)?;
    if stored(reg, project)?
        .iter()
        .any(|s| s.kind == kind && s.target == target)
    {
        return Err((409, "this source is already added".into()));
    }
    let now = crate::procs::now_ms();
    let s = Source {
        id: ulid(now),
        kind: kind.into(),
        target: target.into(),
        implicit: false,
    };
    sql(reg.conn.execute(
        "INSERT INTO task_sources (id, project, kind, target, created_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![s.id, project, s.kind, s.target, now],
    ))?;
    Ok(s)
}

fn add(ctx: &Ctx, b: &Value) -> Res<Source> {
    let project = b["project"]
        .as_str()
        .map(str::trim)
        .filter(|p| Path::new(p).is_absolute())
        .ok_or_else(|| bad("project must be an absolute path"))?;
    insert(
        &db(ctx)?,
        project,
        b["kind"].as_str().unwrap_or(""),
        b["target"].as_str().unwrap_or(""),
    )
}

pub fn delete(reg: &Registry, id: &str) -> Res<()> {
    match sql(reg
        .conn
        .execute("DELETE FROM task_sources WHERE id = ?1", [id]))?
    {
        0 => Err((404, format!("no source {id}"))),
        _ => Ok(()),
    }
}

fn remove(ctx: &Ctx, id: &str) -> Res<()> {
    delete(&db(ctx)?, id)
}

pub fn parse_github(out: &str, source: &str) -> Res<Vec<Value>> {
    let v: Vec<Value> =
        serde_json::from_str(out).map_err(|e| (502, format!("gh returned bad JSON: {e}")))?;
    Ok(v.iter()
        .filter_map(|i| {
            let n = i["number"].as_i64()?;
            Some(json!({
                "source": source,
                "kind": "github",
                "key": format!("#{n}"),
                "title": i["title"].as_str().unwrap_or(""),
                "body": i["body"].as_str().unwrap_or(""),
                "url": i["url"].as_str().unwrap_or(""),
                "taskType": task_type(&i["labels"], |l| l["name"].as_str()),
            }))
        })
        .collect())
}

pub fn parse_gitlab(out: &str, source: &str) -> Res<Vec<Value>> {
    let v: Vec<Value> =
        serde_json::from_str(out).map_err(|e| (502, format!("glab returned bad JSON: {e}")))?;
    Ok(v.iter()
        .filter_map(|i| {
            let n = i["iid"].as_i64()?;
            Some(json!({
                "source": source,
                "kind": "gitlab",
                "key": format!("#{n}"),
                "title": i["title"].as_str().unwrap_or(""),
                "body": i["description"].as_str().unwrap_or(""),
                "url": i["web_url"].as_str().unwrap_or(""),
                "taskType": task_type(&i["labels"], Value::as_str),
            }))
        })
        .collect())
}

fn task_type<'a>(labels: &'a Value, name: impl Fn(&'a Value) -> Option<&'a str>) -> Value {
    let names: Vec<&str> = labels
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(name)
        .collect();
    json!(crate::profiles::task_type_from(&names))
}

pub fn parse_jira(out: &str, source: &str) -> Vec<Value> {
    out.lines()
        .filter_map(|l| {
            let (kind, rest) = l.split_once('\t')?;
            let (key, title) = rest.trim_start_matches('\t').split_once('\t')?;
            let key = key.trim();
            (!key.is_empty()).then(|| {
                json!({
                    "source": source,
                    "kind": "jira",
                    "key": key,
                    "title": title.trim(),
                    "body": "",
                    "url": "",
                    "ticketKey": ticket(key).filter(|t| t == key),
                    "taskType": crate::profiles::task_type_from(&[kind]),
                })
            })
        })
        .collect()
}

fn fetch(s: &Source) -> Res<Vec<Value>> {
    let run = |bin: &str, args: &[&str]| {
        crate::github::run(bin, None, args, TIMEOUT).map_err(|(c, e, _)| (c, e))
    };
    match s.kind.as_str() {
        "github" => parse_github(
            &run(
                "gh",
                &[
                    "issue",
                    "list",
                    "-R",
                    &s.target,
                    "--state",
                    "open",
                    "--limit",
                    LIMIT,
                    "--json",
                    "number,title,body,url,labels",
                ],
            )?,
            &s.id,
        ),
        "gitlab" => parse_gitlab(
            &run(
                "glab",
                &["issue", "list", "-R", &s.target, "-O", "json", "-P", LIMIT],
            )?,
            &s.id,
        ),
        "jira" => Ok(parse_jira(
            &run(
                "jira",
                &[
                    "issue",
                    "list",
                    &format!("--jql={}", s.target),
                    "--plain",
                    "--no-headers",
                    "--columns",
                    "type,key,summary",
                    "--paginate",
                    LIMIT,
                ],
            )?,
            &s.id,
        )),
        k => Err(bad(format!("unknown source kind {k}"))),
    }
}

pub fn items(ctx: &Ctx, project: &str) -> Res<Value> {
    let sources = list(ctx, project)?;
    let results: Vec<(String, Res<Vec<Value>>)> = std::thread::scope(|sc| {
        let hs: Vec<_> = sources
            .iter()
            .map(|s| (s.id.clone(), sc.spawn(|| fetch(s))))
            .collect();
        hs.into_iter()
            .map(|(id, h)| {
                (
                    id,
                    h.join()
                        .unwrap_or_else(|_| Err((500, "source panicked".into()))),
                )
            })
            .collect()
    });
    let mut items = vec![];
    let mut errors = vec![];
    for (id, r) in results {
        match r {
            Ok(v) => items.extend(v),
            Err((_, e)) => errors.push(json!({"source": id, "error": e})),
        }
    }
    Ok(
        json!({"sources": sources.iter().map(Source::to_json).collect::<Vec<_>>(), "items": items, "errors": errors}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(name: &str) -> (std::path::PathBuf, Registry) {
        let dir = std::env::temp_dir().join(format!("sb-src-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let r = Registry::open(&dir).unwrap();
        (dir, r)
    }

    #[test]
    fn sources_are_stored_per_project_and_deduplicated() {
        let (dir, r) = reg("crud");
        let a = insert(&r, "/p", "github", " Acme/widget ").unwrap();
        assert_eq!(a.target, "Acme/widget");
        insert(&r, "/p", "jira", "project = AS").unwrap();
        insert(&r, "/q", "github", "Acme/other").unwrap();
        assert_eq!(
            insert(&r, "/p", "github", "Acme/widget").unwrap_err().0,
            409
        );
        let p = stored(&r, "/p").unwrap();
        assert_eq!(
            p.iter().map(|s| s.kind.as_str()).collect::<Vec<_>>(),
            ["github", "jira"]
        );
        assert_eq!(stored(&r, "/q").unwrap().len(), 1);
        delete(&r, &a.id).unwrap();
        assert_eq!(delete(&r, &a.id).unwrap_err().0, 404);
        assert_eq!(stored(&r, "/p").unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_sources_are_rejected() {
        assert!(validate("github", "acme/widget.rs").is_ok());
        for t in ["", "acme", "acme/", "-R/x", "a/b/c", "../x", "a b/c"] {
            assert_eq!(validate("github", t).unwrap_err().0, 400, "{t}");
        }
        assert!(validate("jira", "").is_err());
        assert!(validate("jira", &"x".repeat(1001)).is_err());
        assert!(validate("bitbucket", "a/b").unwrap_err().1.contains("kind"));
    }

    #[test]
    fn remotes_map_to_an_implicit_source() {
        let r = remote_source;
        assert_eq!(
            r("github.com/Acme/widget"),
            Some(("github", "Acme/widget".into()))
        );
        assert_eq!(r("gitlab.com/g/sub/w"), Some(("gitlab", "g/sub/w".into())));
        assert_eq!(
            r("gitlab.acme.io/g/w"),
            Some(("gitlab", "https://gitlab.acme.io/g/w".into()))
        );
        assert_eq!(r("bitbucket.org/a/b"), None);
        assert_eq!(r("local:/x"), None);
    }

    #[test]
    fn gitlab_targets_take_nested_groups_or_a_host_url() {
        for t in ["g/w", "g/sub/w", "https://gitlab.acme.io/g/w"] {
            assert!(validate("gitlab", t).is_ok(), "{t}");
        }
        for t in ["w", "-R/x", "g//w", "http://h/g/w", "https://h/w", "g/w x"] {
            assert_eq!(validate("gitlab", t).unwrap_err().0, 400, "{t}");
        }
    }

    #[test]
    fn gitlab_output_becomes_items() {
        let out = r#"[{"iid":8709,"title":"T","description":"d","web_url":"https://gitlab.com/g/w/-/issues/8709","labels":["type::bug"]},{"title":"x"}]"#;
        let v = parse_gitlab(out, "gitlab").unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["taskType"], "fix");
        assert_eq!(v[0]["key"], "#8709");
        assert_eq!(v[0]["body"], "d");
        assert_eq!(v[0]["url"], "https://gitlab.com/g/w/-/issues/8709");
        assert_eq!(parse_gitlab("{", "s").unwrap_err().0, 502);
    }

    #[test]
    fn github_output_becomes_items() {
        let out = r#"[{"number":50,"title":"Import","body":"b","url":"https://x/50","labels":[{"name":"enhancement"}]},{"number":51,"title":"Plain"},{"title":"no number"}]"#;
        let v = parse_github(out, "github").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["taskType"], "feature");
        assert!(v[1]["taskType"].is_null());
        assert_eq!(v[0]["key"], "#50");
        assert_eq!(v[0]["title"], "Import");
        assert_eq!(v[0]["url"], "https://x/50");
        assert_eq!(parse_github("[]", "s").unwrap().len(), 0);
        assert_eq!(parse_github("nope", "s").unwrap_err().0, 502);
    }

    #[test]
    fn jira_output_becomes_items() {
        let out = "Bug\tAS-12\tFix login\nStory\t\tOPS-3\tDeploy\tx\nWidget\tAS-9\tOther\n\nbroken line\nAS-1\tno type\n";
        let v = parse_jira(out, "j");
        assert_eq!(v.len(), 3);
        assert_eq!(v[0]["key"], "AS-12");
        assert_eq!(v[0]["title"], "Fix login");
        assert_eq!(v[0]["ticketKey"], "AS-12");
        assert_eq!(v[0]["taskType"], "fix");
        assert_eq!(v[1]["title"], "Deploy\tx");
        assert_eq!(v[1]["ticketKey"], "OPS-3");
        assert_eq!(v[1]["taskType"], "feature");
        assert!(v[2]["taskType"].is_null());
    }

    #[test]
    fn a_missing_cli_is_reported_as_an_error() {
        let s = Source {
            id: "x".into(),
            kind: "github".into(),
            target: "a/b".into(),
            implicit: false,
        };
        let dir = std::env::temp_dir();
        let r = crate::github::run("sb-no-such-bin", Some(&dir), &[], TIMEOUT);
        assert_eq!(r.unwrap_err().0, 424);
        assert_eq!(
            fetch(&Source {
                kind: "x".into(),
                ..s
            })
            .unwrap_err()
            .0,
            400
        );
    }
}
