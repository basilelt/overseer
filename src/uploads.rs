use crate::sys::OpenOptionsExt;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MAX: u64 = 50 * 1024 * 1024;
const KEEP: Duration = Duration::from_secs(7 * 24 * 3600);
const NAME_MAX: usize = 80;

type Res<T> = Result<T, (u16, String)>;

pub fn admit(len: Option<u64>, chunked: bool) -> Res<u64> {
    match len {
        Some(n) if !chunked && n > MAX => Err((413, "upload exceeds 50 MB".into())),
        Some(n) if !chunked => Ok(n),
        _ => Err((411, "Content-Length required".into())),
    }
}

pub fn check_name(raw: &str) -> Result<String, String> {
    let n = raw.trim();
    if n.is_empty() {
        return Err("name required".into());
    }
    if n.contains(['/', '\\', '\0']) || n == "." || n == ".." {
        return Err("name must be a plain file name".into());
    }
    let mut s = String::with_capacity(n.len());
    for c in n.chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
            c
        } else {
            '_'
        };
        if c == '_' && s.ends_with('_') {
            continue;
        }
        s.push(c);
    }
    Ok(cap(s.trim_start_matches('.')))
}

fn cap(s: &str) -> String {
    if s.is_empty() {
        return "file".into();
    }
    if s.len() <= NAME_MAX {
        return s.to_string();
    }
    match s.rfind('.') {
        Some(i) if i > 0 && s.len() - i <= 16 => {
            let ext = &s[i..];
            format!("{}{ext}", &s[..NAME_MAX - ext.len()])
        }
        _ => s[..NAME_MAX].to_string(),
    }
}

pub const SHORT_DIR: &str = ".overseer/u";
const ID_LEN: usize = 6;
const ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

pub fn dir() -> PathBuf {
    crate::home().join(SHORT_DIR)
}

fn id(attempt: u32) -> String {
    let mut b = [0u8; ID_LEN];
    if crate::sys::random(&mut b).is_err() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
            ^ attempt.rotate_left(16)
            ^ std::process::id();
        for (i, x) in b.iter_mut().enumerate() {
            *x = (n >> (i * 5)) as u8;
        }
    }
    b.iter()
        .map(|x| ALPHABET[*x as usize % ALPHABET.len()] as char)
        .collect()
}

pub fn ext(name: &str) -> Option<String> {
    let (stem, e) = name.rsplit_once('.')?;
    (!stem.is_empty()
        && !e.is_empty()
        && e.len() <= 10
        && e.chars().all(|c| c.is_ascii_alphanumeric()))
    .then(|| e.to_ascii_lowercase())
}

pub fn store(dir: &Path, name: &str, len: u64, body: impl Read) -> Res<PathBuf> {
    crate::mkdir_private(dir).map_err(|e| (500, format!("{}: {e}", dir.display())))?;
    let suffix = ext(name).map(|e| format!(".{e}")).unwrap_or_default();
    let mut opened = None;
    for attempt in 0..16 {
        let p = dir.join(format!("{}{suffix}", id(attempt)));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&p)
        {
            Ok(f) => {
                opened = Some((p, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err((500, format!("{}: {e}", p.display()))),
        }
    }
    let (path, mut f) = opened.ok_or_else(|| (500, "no free upload file name".to_string()))?;
    let copied = std::io::copy(&mut body.take(len), &mut f);
    drop(f);
    match copied {
        Ok(n) if n == len => Ok(std::path::absolute(&path).unwrap_or(path)),
        Ok(n) => {
            let _ = std::fs::remove_file(&path);
            Err((400, format!("upload truncated: got {n} of {len} bytes")))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            Err((400, format!("upload failed: {e}")))
        }
    }
}

pub const TASK_DIR: &str = ".overseer/tasks";

pub fn task_dir(id: &str) -> PathBuf {
    crate::home().join(TASK_DIR).join(id)
}

pub fn attachments(uploads: &Path, list: &Value) -> Result<Vec<(PathBuf, String)>, String> {
    let items = match list {
        Value::Null => return Ok(vec![]),
        Value::Array(a) if a.len() <= 20 => a,
        Value::Array(_) => return Err("at most 20 attachments".into()),
        _ => return Err("attachments must be a list".into()),
    };
    let root = uploads.canonicalize().ok();
    items
        .iter()
        .map(|it| {
            let raw = it["path"].as_str().ok_or("attachment path required")?;
            let path = Path::new(raw)
                .canonicalize()
                .map_err(|_| format!("{raw}: no such upload"))?;
            if root.is_none() || path.parent() != root.as_deref() || !path.is_file() {
                return Err(format!("{raw} is not an upload"));
            }
            let fallback = path.file_name().unwrap_or_default().to_string_lossy();
            let name = it["name"]
                .as_str()
                .and_then(|n| check_name(n).ok())
                .unwrap_or_else(|| fallback.into_owned());
            Ok((path, name))
        })
        .collect()
}

pub fn keep(dest: &Path, files: &[(PathBuf, String)]) -> Result<Vec<PathBuf>, String> {
    crate::mkdir_private(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    files
        .iter()
        .enumerate()
        .map(|(i, (from, name))| {
            let mut to = dest.join(name);
            if to.exists() {
                to = dest.join(format!("{}-{name}", i + 1));
            }
            std::fs::rename(from, &to)
                .or_else(|_| std::fs::copy(from, &to).and_then(|_| std::fs::remove_file(from)))
                .map_err(|e| format!("{}: {e}", from.display()))?;
            Ok(to)
        })
        .collect()
}

pub fn short(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) => format!("~/{}", rel.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

pub fn receive(dir: &Path, name: Option<&str>, len: u64, body: impl Read) -> Res<Value> {
    let name = check_name(&crate::board::decode(name.unwrap_or_default())).map_err(|e| (400, e))?;
    let path = store(dir, &name, len, body)?;
    Ok(json!({
        "path": path.to_string_lossy(),
        "short": short(&path, &crate::home()),
        "name": name,
        "size": len,
    }))
}

fn sweep(dir: &Path, now: SystemTime, keep_dir: bool) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    for f in files.flatten() {
        let old = f
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|m| now.duration_since(m).is_ok_and(|age| age > KEEP));
        if old && f.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(f.path());
        }
    }
    if !keep_dir {
        let _ = std::fs::remove_dir(dir);
    }
}

pub fn cleanup(dir: &Path, legacy: &Path, now: SystemTime) {
    sweep(dir, now, true);
    let Ok(days) = std::fs::read_dir(legacy) else {
        return;
    };
    for d in days
        .flatten()
        .filter(|d| d.file_type().is_ok_and(|t| t.is_dir()))
    {
        sweep(&d.path(), now, false);
    }
    let _ = std::fs::remove_dir(legacy);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "sb-uploads-{tag}-{}-{}",
            std::process::id(),
            crate::procs::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sanitises_names() {
        let ok = |s: &str| check_name(s).unwrap();
        assert_eq!(ok("report.pdf"), "report.pdf");
        assert_eq!(ok("  my  file (1).png "), "my_file_1_.png");
        assert_eq!(ok("résumé été.txt"), "r_sum_t_.txt");
        assert_eq!(ok(".bashrc"), "bashrc");
        assert_eq!(ok("...x"), "x");
        assert_eq!(ok("..."), "file");
        assert_eq!(ok("日本"), "_");
        let long = format!("{}.tar.gz", "a".repeat(200));
        let s = ok(&long);
        assert_eq!(s.len(), 80);
        assert!(s.ends_with(".gz"), "{s}");
        assert_eq!(ok(&"b".repeat(200)), "b".repeat(80));
    }

    #[test]
    fn refuses_traversal() {
        for bad in [
            "../../etc/passwd",
            "..",
            ".",
            "a/b",
            "c:\\x",
            "",
            "   ",
            "a\0b",
        ] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            check_name(&crate::board::decode("..%2F..%2Fetc%2Fpasswd")),
            Err("name must be a plain file name".into())
        );
    }

    #[test]
    fn admits_by_length() {
        assert_eq!(admit(Some(10), false), Ok(10));
        assert_eq!(admit(Some(MAX), false), Ok(MAX));
        assert_eq!(admit(Some(MAX + 1), false).unwrap_err().0, 413);
        assert_eq!(admit(None, false).unwrap_err().0, 411);
        assert_eq!(admit(Some(10), true).unwrap_err().0, 411);
    }

    #[test]
    fn stores_under_a_short_name_keeping_the_extension() {
        let d = tmp("store").join("u");
        let body: &[u8] = b"hello world, extra bytes ignored";
        let p = store(&d, "Screen_Shot.PNG", 11, body).unwrap();
        assert!(p.is_absolute());
        assert_eq!(std::fs::read(&p).unwrap(), b"hello world");
        assert_eq!(p.parent().unwrap(), d);
        let f = p.file_name().unwrap().to_string_lossy().into_owned();
        let (id, e) = f.split_once('.').unwrap();
        assert_eq!(e, "png");
        assert_eq!(id.len(), 6);
        assert!(id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        let bare = store(&d, "Makefile", 1, &b"x"[..]).unwrap();
        assert_eq!(bare.file_name().unwrap().len(), 6);
        #[cfg(unix)]
        {
            use crate::sys::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&p), 0o600);
            assert_eq!(mode(&d), 0o700);
        }
        let v = receive(&d, Some("a%20b.txt"), 3, &b"xyz"[..]).unwrap();
        assert_eq!(v["name"], "a_b.txt");
        assert_eq!(v["size"], 3);
        assert_eq!(std::fs::read(v["path"].as_str().unwrap()).unwrap(), b"xyz");
        assert!(v["path"].as_str().unwrap().ends_with(".txt"));
        assert_eq!(receive(&d, Some(".."), 1, &b"x"[..]).unwrap_err().0, 400);
        let _ = std::fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn shortens_paths_under_home() {
        let home = Path::new("/home/u");
        assert_eq!(
            short(Path::new("/home/u/.overseer/u/a1b2c3.png"), home),
            "~/.overseer/u/a1b2c3.png"
        );
        assert_eq!(short(Path::new("/srv/x.png"), home), "/srv/x.png");
        assert_eq!(ext("a.tar.GZ").as_deref(), Some("gz"));
        assert_eq!(ext("noext"), None);
        assert_eq!(ext("bashrc."), None);
        assert_eq!(ext("x.not-an-ext"), None);
    }

    #[test]
    fn short_body_leaves_nothing() {
        let d = tmp("short").join("u");
        let e = store(&d, "x.bin", 100, &b"only a few"[..]).unwrap_err();
        assert_eq!(e.0, 400);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn cleanup_drops_old_files_and_empty_legacy_days() {
        let d = tmp("cleanup");
        let now = SystemTime::now();
        let short = d.join("u");
        let legacy = d.join("uploads");
        let old_day = legacy.join("20200101");
        let mixed = legacy.join("20200102");
        for x in [&short, &old_day, &mixed] {
            std::fs::create_dir_all(x).unwrap();
        }
        let stale = now - Duration::from_secs(8 * 24 * 3600);
        let write = |p: PathBuf, t: SystemTime| {
            let f = std::fs::File::create(&p).unwrap();
            f.set_modified(t).unwrap();
            p
        };
        let a = write(old_day.join("aaaaaaaa-old.txt"), stale);
        let b = write(mixed.join("bbbbbbbb-old.txt"), stale);
        let c = write(mixed.join("cccccccc-new.txt"), now);
        let s_old = write(short.join("abcdef.png"), stale);
        let s_new = write(short.join("ghijkl.png"), now);
        cleanup(&short, &legacy, now);
        assert!(!a.exists());
        assert!(!b.exists());
        assert!(c.exists());
        assert!(!old_day.exists());
        assert!(mixed.exists());
        assert!(!s_old.exists());
        assert!(s_new.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn attachments_accept_only_files_in_the_upload_dir() {
        let d = tmp("attach");
        let up = d.join("u");
        std::fs::create_dir_all(&up).unwrap();
        std::fs::write(up.join("ab12cd.png"), "png").unwrap();
        std::fs::write(d.join("secret"), "x").unwrap();
        let p = |s: &str| up.join(s).to_string_lossy().into_owned();
        assert!(attachments(&up, &Value::Null).unwrap().is_empty());
        let ok = attachments(
            &up,
            &json!([{"path": p("ab12cd.png"), "name": "my shot.png"}, {"path": p("../u/ab12cd.png")}]),
        )
        .unwrap();
        assert_eq!(ok[0].1, "my_shot.png");
        assert_eq!(ok[1].1, "ab12cd.png");
        assert_eq!(ok[0].0, up.join("ab12cd.png").canonicalize().unwrap());
        for bad in [
            json!("x"),
            json!([{"name": "a"}]),
            json!([{"path": p("missing.png")}]),
            json!([{"path": d.join("secret").to_string_lossy()}]),
            json!([{"path": p("../secret")}]),
            json!([{"path": up.to_string_lossy()}]),
            Value::Array(vec![json!({"path": p("ab12cd.png")}); 21]),
        ] {
            assert!(attachments(&up, &bad).is_err(), "{bad}");
        }
        assert!(attachments(&d.join("none"), &json!([{"path": p("ab12cd.png")}])).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn keep_moves_uploads_and_renames_clashes() {
        let d = tmp("keep");
        let a = d.join("a.png");
        let b = d.join("b.png");
        std::fs::write(&a, "A").unwrap();
        std::fs::write(&b, "B").unwrap();
        let dest = d.join("task");
        let kept = keep(
            &dest,
            &[(a.clone(), "x.png".into()), (b.clone(), "x.png".into())],
        )
        .unwrap();
        assert_eq!(kept, [dest.join("x.png"), dest.join("2-x.png")]);
        assert_eq!(std::fs::read(&kept[0]).unwrap(), b"A");
        assert_eq!(std::fs::read(&kept[1]).unwrap(), b"B");
        assert!(!a.exists() && !b.exists());
        assert!(keep(&dest, &[(d.join("gone"), "g".into())]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
