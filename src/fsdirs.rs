use crate::http::{err, respond_raw, Request};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;

pub const CAP: usize = 2000;

type Res<T> = Result<T, (u16, String)>;

pub fn serve(r: &Request, s: &mut dyn Write) -> bool {
    if r.method != "GET" || r.path.trim_matches('/') != "v1/fs/dirs" {
        return false;
    }
    let raw = r.param("path").unwrap_or_default();
    let (code, v) = match list(&raw, r.is_one("hidden"), CAP) {
        Ok(v) => (200, v),
        Err((code, e)) => err(code, e),
    };
    respond_raw(s, code, "application/json", v.to_string().as_bytes());
    true
}

pub fn list(raw: &str, hidden: bool, cap: usize) -> Res<Value> {
    let raw = raw.trim();
    let p = if raw.is_empty() {
        crate::home()
    } else {
        crate::files::resolve(raw, None)
            .map_err(|_| (400, format!("{raw}: use an absolute path or ~")))?
    };
    let p = p.canonicalize().map_err(|e| crate::files::io(&p, e))?;
    if !p.is_dir() {
        return Err((400, format!("{} is not a directory", p.display())));
    }
    let mut dirs: Vec<(String, PathBuf)> = std::fs::read_dir(&p)
        .map_err(|e| crate::files::io(&p, e))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !hidden && name.starts_with('.') {
                return None;
            }
            let path = e.path();
            let ft = e.file_type().ok()?;
            (ft.is_dir() || (ft.is_symlink() && path.is_dir())).then_some((name, path))
        })
        .collect();
    dirs.sort_by_cached_key(|(n, _)| (n.to_lowercase(), n.clone()));
    let truncated = dirs.len() > cap;
    dirs.truncate(cap);
    let dirs: Vec<Value> = dirs
        .into_iter()
        .map(|(name, path)| {
            let git = path.join(".git").symlink_metadata().is_ok();
            json!({ "name": name, "path": path.to_string_lossy(), "git": git })
        })
        .collect();
    let home = crate::home();
    let home = home.canonicalize().unwrap_or(home);
    Ok(json!({
        "path": p.to_string_lossy(),
        "parent": p.parent().map(|x| x.to_string_lossy().into_owned()),
        "home": home.to_string_lossy(),
        "dirs": dirs,
        "truncated": truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-fsdirs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    fn names(v: &Value) -> Vec<&str> {
        v["dirs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn lists_only_dirs_sorted_without_hidden() {
        let d = tmp("list");
        for n in ["beta", "Alpha", "gamma", ".hidden", "alpha2"] {
            std::fs::create_dir(d.join(n)).unwrap();
        }
        std::fs::write(d.join("file.txt"), "x").unwrap();
        let v = list(d.to_str().unwrap(), false, CAP).unwrap();
        assert_eq!(names(&v), ["Alpha", "alpha2", "beta", "gamma"]);
        assert_eq!(v["path"], d.to_string_lossy().as_ref());
        assert_eq!(v["parent"], d.parent().unwrap().to_string_lossy().as_ref());
        assert_eq!(v["truncated"], false);
        assert_eq!(
            v["dirs"][0]["path"],
            d.join("Alpha").to_string_lossy().as_ref()
        );
        let v = list(d.to_str().unwrap(), true, CAP).unwrap();
        assert_eq!(names(&v), [".hidden", "Alpha", "alpha2", "beta", "gamma"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn git_flag_symlinks_and_cap() {
        let d = tmp("git");
        std::fs::create_dir_all(d.join("repo/.git")).unwrap();
        std::fs::create_dir(d.join("wt")).unwrap();
        std::fs::write(d.join("wt/.git"), "gitdir: x").unwrap();
        std::fs::create_dir(d.join("plain")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.join("plain"), d.join("link")).unwrap();
            std::os::unix::fs::symlink(d.join("nope"), d.join("dangling")).unwrap();
        }
        let v = list(d.to_str().unwrap(), false, CAP).unwrap();
        let git: Vec<(&str, bool)> = v["dirs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (x["name"].as_str().unwrap(), x["git"].as_bool().unwrap()))
            .collect();
        #[cfg(unix)]
        assert_eq!(
            git,
            [
                ("link", false),
                ("plain", false),
                ("repo", true),
                ("wt", true)
            ]
        );
        let v = list(d.to_str().unwrap(), false, 2).unwrap();
        assert_eq!(v["dirs"].as_array().unwrap().len(), 2);
        assert_eq!(v["truncated"], true);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn home_default_and_errors() {
        let home = crate::home().canonicalize().unwrap();
        for raw in ["", "~", " ~ "] {
            let v = list(raw, false, 1).unwrap();
            assert_eq!(v["path"], home.to_string_lossy().as_ref(), "{raw:?}");
            assert_eq!(v["home"], home.to_string_lossy().as_ref());
        }
        let root = list("/", false, CAP).unwrap();
        assert_eq!(root["parent"], Value::Null);
        let d = tmp("err");
        std::fs::write(d.join("f"), "x").unwrap();
        assert_eq!(
            list(d.join("nope").to_str().unwrap(), false, CAP)
                .unwrap_err()
                .0,
            404
        );
        assert_eq!(
            list(d.join("f").to_str().unwrap(), false, CAP)
                .unwrap_err()
                .0,
            400
        );
        assert_eq!(list("relative/dir", false, CAP).unwrap_err().0, 400);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = d.join("locked");
            std::fs::create_dir(&locked).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            if std::fs::read_dir(&locked).is_err() {
                assert_eq!(
                    list(locked.to_str().unwrap(), false, CAP).unwrap_err().0,
                    403
                );
            }
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
