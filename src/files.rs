use crate::http::{respond_err, respond_raw, Request};
use crate::procs::{Probe, System};
use crate::Ctx;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

pub const MAX: u64 = 50 << 20;

type Res<T> = Result<T, (u16, String)>;

pub fn serve(ctx: &Ctx, r: &Request, s: &mut dyn Write) -> bool {
    if r.method != "GET" || r.path.trim_matches('/') != "v1/file" {
        return false;
    }
    match load(ctx, r) {
        Ok((ctype, bytes)) => respond_raw(s, 200, ctype, &bytes),
        Err((code, e)) => respond_err(s, code, &e),
    }
    true
}

fn load(ctx: &Ctx, r: &Request) -> Res<(&'static str, Vec<u8>)> {
    let raw = r.param("path").unwrap_or_default();
    if raw.is_empty() {
        return Err((400, "path required".into()));
    }
    let base = r
        .param("term")
        .and_then(|t| term_cwd(&t))
        .or_else(|| r.param("line").and_then(|l| line_cwd(ctx, &l)));
    let p = resolve(&raw, base.as_deref())?;
    let p = check(&p, MAX)?;
    read(&p, MAX).map(|b| (ctype(&p), b))
}

fn term_cwd(id: &str) -> Option<String> {
    let i = crate::term::get(id)?.info();
    let live = (!i.exited && i.pid > 0)
        .then(|| System.cwd(i.pid))
        .flatten();
    Some(live.unwrap_or(i.cwd))
}

fn line_cwd(ctx: &Ctx, id: &str) -> Option<String> {
    let lines = crate::daemon::cached_lines(ctx, Some(id)).ok()?;
    let l = crate::actions::find_line(&lines, id).ok()?;
    let cwd = if l.cwd.is_empty() {
        &l.worktree_path
    } else {
        &l.cwd
    };
    (!cwd.is_empty()).then(|| cwd.clone())
}

pub(crate) fn io(p: &Path, e: std::io::Error) -> (u16, String) {
    let code = match e.kind() {
        ErrorKind::NotFound => 404,
        ErrorKind::PermissionDenied => 403,
        _ => 400,
    };
    (code, format!("{}: {e}", p.display()))
}

pub fn resolve(raw: &str, base: Option<&str>) -> Res<PathBuf> {
    let p = match raw.strip_prefix('~') {
        Some("") => crate::home(),
        Some(rest) if rest.starts_with('/') => crate::home().join(&rest[1..]),
        _ => PathBuf::from(raw),
    };
    if p.is_absolute() {
        return Ok(p);
    }
    base.map(|b| Path::new(b).join(&p))
        .ok_or((400, "relative path needs a known term".to_string()))
}

pub fn check(p: &Path, max: u64) -> Res<PathBuf> {
    let p = p.canonicalize().map_err(|e| io(p, e))?;
    let m = p.metadata().map_err(|e| io(&p, e))?;
    if !m.is_file() {
        return Err((400, format!("{} is not a regular file", p.display())));
    }
    if m.len() > max {
        return Err((413, too_big(m.len(), max)));
    }
    Ok(p)
}

fn read(p: &Path, max: u64) -> Res<Vec<u8>> {
    let mut b = Vec::new();
    std::fs::File::open(p)
        .and_then(|f| f.take(max + 1).read_to_end(&mut b))
        .map_err(|e| io(p, e))?;
    if b.len() as u64 > max {
        return Err((413, too_big(b.len() as u64, max)));
    }
    Ok(b)
}

fn too_big(len: u64, max: u64) -> String {
    format!("file is {len} bytes, over the {max} byte limit")
}

pub fn ctype(p: &Path) -> &'static str {
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        "md" | "markdown" | "txt" | "log" | "csv" | "tsv" | "json" | "jsonl" | "yaml" | "yml"
        | "toml" | "xml" | "html" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-files-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ctype_by_extension() {
        for (f, t) in [
            ("a.PNG", "image/png"),
            ("a.jpeg", "image/jpeg"),
            ("a.svg", "image/svg+xml"),
            ("a.mov", "video/quicktime"),
            ("a.opus", "audio/ogg"),
            ("a.pdf", "application/pdf"),
            ("a.md", "text/plain; charset=utf-8"),
            ("a.html", "text/plain; charset=utf-8"),
            ("a.bin", "application/octet-stream"),
            ("Makefile", "application/octet-stream"),
        ] {
            assert_eq!(ctype(Path::new(f)), t, "{f}");
        }
    }

    #[test]
    fn resolve_paths() {
        let home = crate::home();
        assert_eq!(resolve("~", None).unwrap(), home);
        assert_eq!(resolve("~/x/y", None).unwrap(), home.join("x/y"));
        assert_eq!(resolve("/a/b", Some("/c")).unwrap(), PathBuf::from("/a/b"));
        assert_eq!(resolve("b", Some("/c")).unwrap(), PathBuf::from("/c/b"));
        assert_eq!(resolve("~x", Some("/c")).unwrap(), PathBuf::from("/c/~x"));
        assert_eq!(resolve("b", None).unwrap_err().0, 400);
    }

    #[test]
    fn check_rejects_dirs_missing_and_oversize() {
        let d = tmp("check");
        assert_eq!(check(&d, MAX).unwrap_err().0, 400);
        assert_eq!(check(&d.join("nope"), MAX).unwrap_err().0, 404);
        let f = d.join("big.bin");
        std::fs::File::create(&f).unwrap().set_len(MAX + 1).unwrap();
        let (code, e) = check(&f, MAX).unwrap_err();
        assert_eq!(code, 413);
        assert!(e.contains(&(MAX + 1).to_string()), "{e}");
        std::fs::File::create(&f).unwrap().set_len(MAX).unwrap();
        assert!(check(&f, MAX).is_ok());
        assert_eq!(read(&f, 4).unwrap_err().0, 413);
        let _ = std::fs::remove_dir_all(&d);
    }
}
