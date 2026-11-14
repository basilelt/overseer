use crate::exec::{output_within, run_within, RunError};
use crate::util::normalize_remote;
use crate::util::now_ms;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;
use std::time::Duration;

pub const TIMEOUT: Duration = Duration::from_secs(10);
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(600);
const INFO_TTL_MS: i64 = 300_000;
const MISS_TTL_MS: i64 = 30_000;
const MAX_CACHED: usize = 4096;

#[derive(Clone, Debug, PartialEq)]
pub struct WtInfo {
    pub top: String,
    pub common: String,
    pub repo: String,
    pub branch: Option<String>,
    pub email: Option<String>,
}

#[derive(Clone)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub prunable: bool,
}

#[derive(Default)]
pub struct Git {
    cache: HashMap<String, Option<WtInfo>>,
}

pub fn limit_for(args: &[&str]) -> Duration {
    let mut it = args.iter();
    let sub = loop {
        match it.next() {
            Some(&"-c") | Some(&"-C") => {
                it.next();
            }
            Some(a) if a.starts_with('-') => {}
            Some(a) => break *a,
            None => break "",
        }
    };
    match sub {
        "commit" | "merge" | "fetch" | "push" | "pull" | "rebase" | "stash" | "worktree"
        | "checkout" | "switch" | "cherry-pick" | "am" | "clone" | "gc" | "revert" => {
            if sub == "worktree" && args.contains(&"list") {
                TIMEOUT
            } else {
                SLOW_TIMEOUT
            }
        }
        _ => TIMEOUT,
    }
}

pub fn git_env(cmd: &mut Command) -> &mut Command {
    cmd.env("GIT_TERMINAL_PROMPT", "0")
}

pub fn git_read_env(cmd: &mut Command) -> &mut Command {
    git_env(cmd).env("GIT_OPTIONAL_LOCKS", "0")
}

pub fn gh_env(cmd: &mut Command) -> &mut Command {
    cmd.env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
}

pub fn run_in(program: &str, dir: &str, args: &[&str], limit: Duration) -> Result<Output, String> {
    output_within(
        git_read_env(Command::new(program).arg("-C").arg(dir).args(args)),
        limit,
    )
}

pub fn run_gh<I, S>(args: I, limit: Duration) -> Result<Output, RunError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    run_within(gh_env(Command::new("gh").args(args)), limit)
}

pub fn git_out(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = output_within(
        git_env(
            Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "color.ui=never", "-c", "core.quotepath=off"])
                .args(args),
        ),
        limit_for(args),
    )
    .map_err(|e| format!("git {}: {e}", args.first().unwrap_or(&"")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = match run_in("git", dir, args, limit_for(args)) {
        Ok(o) => o,
        Err(e) => {
            if e.starts_with("timed out") {
                crate::dlog::line(
                    "warn",
                    &format!(
                        "git {} in {dir} {e}",
                        args.first().copied().unwrap_or_default()
                    ),
                );
            }
            return None;
        }
    };
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

fn mtime(p: &Path) -> Option<i128> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(
        t.duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i128),
    )
}

type Stamp = (Option<i128>, Option<i128>);

struct Cached {
    info: Option<WtInfo>,
    git_dir: Option<PathBuf>,
    stamp: Stamp,
    at_ms: i64,
}

fn stamp(git_dir: &Path, common: &str) -> Stamp {
    (
        mtime(&git_dir.join("HEAD")),
        mtime(&Path::new(common).join("config")),
    )
}

static INFOS: Mutex<Option<HashMap<String, Cached>>> = Mutex::new(None);

fn cached_info(cwd: &str, now: i64) -> Option<Option<WtInfo>> {
    let g = INFOS.lock().unwrap_or_else(|e| e.into_inner());
    let c = g.as_ref()?.get(cwd)?;
    let fresh = match (&c.info, &c.git_dir) {
        (Some(i), Some(d)) => {
            now - c.at_ms < INFO_TTL_MS
                && c.stamp.0.is_some()
                && stamp(d, &i.common) == c.stamp
                && Path::new(cwd).is_dir()
        }
        _ => now - c.at_ms < MISS_TTL_MS && miss_stamp(cwd) == c.stamp,
    };
    fresh.then(|| c.info.clone())
}

fn remember(cwd: &str, c: Cached) {
    let mut g = INFOS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() >= MAX_CACHED {
        m.clear();
    }
    m.insert(cwd.to_string(), c);
}

fn miss_stamp(cwd: &str) -> Stamp {
    let d = Path::new(cwd);
    (mtime(d), mtime(&d.join(".git")))
}

impl Git {
    pub fn info(&mut self, cwd: &str) -> Option<WtInfo> {
        if let Some(v) = self.cache.get(cwd) {
            return v.clone();
        }
        let now = now_ms();
        let v = match cached_info(cwd, now) {
            Some(v) => v,
            None => {
                let before_miss = miss_stamp(cwd);
                let before = git_dirs(cwd).map(|(d, c)| stamp(&d, &c.to_string_lossy()));
                let (v, git_dir) = Self::lookup(cwd);
                let st = match (&v, &git_dir) {
                    (Some(i), Some(d)) => {
                        let after = stamp(d, &i.common);
                        if before.is_none_or(|b| b == after) {
                            after
                        } else {
                            (None, None)
                        }
                    }
                    _ => before_miss,
                };
                remember(
                    cwd,
                    Cached {
                        info: v.clone(),
                        git_dir,
                        stamp: st,
                        at_ms: now,
                    },
                );
                v
            }
        };
        self.cache.insert(cwd.to_string(), v.clone());
        v
    }

    fn lookup(cwd: &str) -> (Option<WtInfo>, Option<PathBuf>) {
        if !Path::new(cwd).is_dir() {
            return (None, None);
        }
        let Some(out) = git(
            cwd,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--git-common-dir",
                "--git-dir",
            ],
        ) else {
            return (None, None);
        };
        let mut lines = out.lines();
        let (Some(top), Some(common)) = (lines.next(), lines.next()) else {
            return (None, None);
        };
        let (top, common) = (top.to_string(), common.to_string());
        let git_dir = lines.next().map(PathBuf::from);
        let repo = git(&top, &["config", "--get", "remote.origin.url"])
            .and_then(|u| normalize_remote(&u))
            .unwrap_or_else(|| {
                let main = common.strip_suffix("/.git").unwrap_or(&common);
                format!("local:{main}")
            });
        let branch =
            git(&top, &["symbolic-ref", "--short", "-q", "HEAD"]).filter(|b| !b.is_empty());
        let email = git(&top, &["config", "user.email"]).filter(|e| !e.is_empty());
        (
            Some(WtInfo {
                top,
                common,
                repo,
                branch,
                email,
            }),
            git_dir,
        )
    }
}

type ListStamp = Vec<(String, Option<i128>, bool)>;

type Lists = HashMap<String, (ListStamp, i64, Vec<Worktree>)>;

static LISTS: Mutex<Option<Lists>> = Mutex::new(None);

fn list_stamp(dir: &str, cached: &[Worktree]) -> Option<ListStamp> {
    let common = git_common(dir)?;
    let mut s: ListStamp = vec![
        ("HEAD".into(), mtime(&common.join("HEAD")), false),
        ("worktrees".into(), mtime(&common.join("worktrees")), false),
    ];
    if let Ok(rd) = std::fs::read_dir(common.join("worktrees")) {
        let mut names: Vec<_> = rd
            .flatten()
            .map(|e| {
                let p = e.path();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    mtime(&p.join("HEAD")),
                    p.join("locked").exists(),
                )
            })
            .collect();
        names.sort();
        s.extend(names);
    }
    s.extend(
        cached
            .iter()
            .map(|w| (w.path.clone(), None, Path::new(&w.path).exists())),
    );
    Some(s)
}

pub fn git_dirs(dir: &str) -> Option<(PathBuf, PathBuf)> {
    let dot = Path::new(dir).join(".git");
    if dot.is_dir() {
        return Some((dot.clone(), dot));
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let gd = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    let gd = if gd.is_absolute() {
        gd
    } else {
        Path::new(dir).join(gd)
    };
    let common = std::fs::read_to_string(gd.join("commondir"))
        .ok()
        .map(|c| {
            let c = PathBuf::from(c.trim());
            if c.is_absolute() {
                c
            } else {
                gd.join(c)
            }
        })
        .unwrap_or_else(|| gd.clone());
    Some((gd, common))
}

fn git_common(dir: &str) -> Option<PathBuf> {
    git_dirs(dir).map(|d| d.1)
}

pub type RefStamp = Vec<Option<i128>>;

pub fn ref_stamp(wt: &str, branch: Option<&str>) -> Option<RefStamp> {
    let (gd, common) = git_dirs(wt)?;
    let mut paths = vec![
        gd.join("HEAD"),
        gd.join("logs/HEAD"),
        common.join("config"),
        common.join("packed-refs"),
        common.join("refs/heads"),
        common.join("refs/remotes/origin"),
        common.join("refs/remotes/origin/HEAD"),
    ];
    for b in branch
        .into_iter()
        .chain(["main", "master", "develop", "trunk"])
    {
        paths.push(common.join("refs/heads").join(b));
        paths.push(common.join("logs/refs/heads").join(b));
        paths.push(common.join("refs/remotes/origin").join(b));
    }
    Some(paths.iter().map(|p| mtime(p)).collect())
}

pub fn worktrees(dir: &str) -> Vec<Worktree> {
    let now = now_ms();
    let hit = LISTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(dir))
        .filter(|c| now - c.1 < INFO_TTL_MS)
        .map(|c| (c.0.clone(), c.2.clone()));
    if let Some((st, list)) = hit {
        if list_stamp(dir, &list).as_ref() == Some(&st) {
            return list;
        }
    }
    let list = list_worktrees(dir);
    if let Some(st) = list_stamp(dir, &list) {
        let mut g = LISTS.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        if m.len() >= MAX_CACHED {
            m.clear();
        }
        m.insert(dir.to_string(), (st, now, list.clone()));
    }
    list
}

fn list_worktrees(dir: &str) -> Vec<Worktree> {
    let Some(out) = git(dir, &["worktree", "list", "--porcelain"]) else {
        return vec![];
    };
    let mut res: Vec<Worktree> = vec![];
    for line in out.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            res.push(Worktree {
                path: p.to_string(),
                branch: None,
                prunable: false,
            });
        } else if let Some(w) = res.last_mut() {
            if let Some(b) = line.strip_prefix("branch ") {
                w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line.starts_with("prunable") {
                w.prunable = true;
            }
        }
    }
    res
}

#[cfg(test)]
pub mod fixture {
    use std::path::Path;
    use std::process::Command;

    pub fn cmd(dir: &Path) -> Command {
        let mut c = Command::new("git");
        c.current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=T",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "protocol.file.allow=always",
                "-c",
                "init.defaultBranch=main",
            ]);
        c
    }

    pub fn checked(mut c: Command, args: &[&str]) -> String {
        let o = c.args(args).output().unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    pub fn run(dir: &Path, args: &[&str]) -> String {
        checked(cmd(dir), args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_git_is_killed_with_its_process_group() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("hang");
        let pid = d.join("pid");
        let fake = d.join("slow-git");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nsleep 300 &\necho $! > {}\nwait\n",
                pid.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let begin = std::time::Instant::now();
        let e = (0..20)
            .map(|_| {
                run_in(
                    fake.to_str().unwrap(),
                    d.to_str().unwrap(),
                    &["status"],
                    Duration::from_millis(300),
                )
                .unwrap_err()
            })
            .find(|e| !e.contains("os error 26"))
            .unwrap();
        assert!(e.starts_with("timed out"), "{e}");
        assert!(begin.elapsed() < Duration::from_secs(5));
        let child: i32 = std::fs::read_to_string(&pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let gone = (0..100).any(|_| {
            let alive = crate::procs::alive_not_zombie(child);
            if alive {
                std::thread::sleep(Duration::from_millis(20));
            }
            !alive
        });
        assert!(gone, "sleep {child} outlived the timeout");
        let fast = run_in("/bin/echo", "x", &["ok"], Duration::from_secs(5)).unwrap();
        assert_eq!(String::from_utf8_lossy(&fast.stdout).trim(), "-C x ok");
        let _ = std::fs::remove_dir_all(d);
    }

    fn env_of(c: &Command, k: &str) -> Option<String> {
        c.get_envs()
            .find(|(n, _)| *n == k)
            .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
    }

    #[test]
    fn each_tool_gets_its_own_environment() {
        let mut g = Command::new("git");
        git_env(&mut g);
        assert_eq!(env_of(&g, "GIT_TERMINAL_PROMPT").as_deref(), Some("0"));
        assert_eq!(env_of(&g, "GIT_OPTIONAL_LOCKS"), None);
        let mut r = Command::new("git");
        git_read_env(&mut r);
        assert_eq!(env_of(&r, "GIT_OPTIONAL_LOCKS").as_deref(), Some("0"));
        let mut h = Command::new("gh");
        gh_env(&mut h);
        for k in ["GH_PROMPT_DISABLED", "GH_NO_UPDATE_NOTIFIER", "NO_COLOR"] {
            assert_eq!(env_of(&h, k).as_deref(), Some("1"), "{k}");
        }
        assert_eq!(env_of(&h, "GIT_TERMINAL_PROMPT"), None);
    }

    #[test]
    fn git_out_names_the_subcommand_in_both_failure_shapes() {
        let d = tmp("out");
        let dir = d.to_string_lossy().into_owned();
        let e = git_out(&dir, &["rev-parse", "HEAD"]).unwrap_err();
        assert!(e.starts_with("git rev-parse: "), "{e}");
        let gone = d.join("nope").to_string_lossy().into_owned();
        assert!(git_out(&gone, &["status"])
            .unwrap_err()
            .starts_with("git status: "));
        sh(&d, &["init", "-q", "-b", "main"]);
        assert_eq!(
            git_out(&dir, &["symbolic-ref", "HEAD"]).unwrap().trim(),
            "refs/heads/main"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn slow_subcommands_get_the_long_limit() {
        assert_eq!(limit_for(&["status", "--porcelain"]), TIMEOUT);
        assert_eq!(limit_for(&["-c", "x=y", "commit", "-m", "m"]), SLOW_TIMEOUT);
        assert_eq!(limit_for(&["worktree", "list", "--porcelain"]), TIMEOUT);
        assert_eq!(limit_for(&["worktree", "add", "p"]), SLOW_TIMEOUT);
        assert_eq!(limit_for(&[]), TIMEOUT);
    }

    fn sh(dir: &Path, args: &[&str]) {
        fixture::run(dir, args);
    }

    #[test]
    fn a_folder_seen_mid_init_is_looked_up_again_once_the_repo_exists() {
        let d = tmp("midinit");
        let dir = d.canonicalize().unwrap().join("r");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let p = dir.to_string_lossy().into_owned();
        assert!(Git::default().info(&p).is_none());
        assert!(Git::default().info(&p).is_none());
        std::thread::sleep(Duration::from_millis(10));
        sh(&dir, &["init", "-q", "-b", "main"]);
        assert_eq!(Git::default().info(&p).map(|i| i.top), Some(p.clone()));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn info_and_worktree_lists_follow_branch_switches() {
        let d = tmp("info");
        let repo = d.join("r");
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "main"]);
        sh(&repo, &["config", "user.email", "t@e.x"]);
        sh(&repo, &["config", "user.name", "t"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "c"]);
        let top = repo.canonicalize().unwrap().to_string_lossy().into_owned();
        let i = Git::default().info(&top).unwrap();
        assert_eq!(i.branch.as_deref(), Some("main"));
        assert_eq!(Git::default().info(&top), Some(i));
        assert_eq!(worktrees(&top).len(), 1);
        std::thread::sleep(Duration::from_millis(20));
        sh(&repo, &["checkout", "-q", "-b", "feat/x"]);
        assert_eq!(
            Git::default().info(&top).unwrap().branch.as_deref(),
            Some("feat/x")
        );
        let wt = d.join("wt");
        sh(
            &repo,
            &["worktree", "add", "-q", "-b", "other", wt.to_str().unwrap()],
        );
        let list = worktrees(&top);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].branch.as_deref(), Some("feat/x"));
        assert_eq!(list[1].branch.as_deref(), Some("other"));
        std::fs::remove_dir_all(&wt).unwrap();
        assert!(worktrees(&top)[1].prunable);
        let _ = std::fs::remove_dir_all(d);
    }
}
