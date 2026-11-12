use std::path::{Path, PathBuf};

pub fn task_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "task".into()
    } else {
        out
    }
}

pub fn branch_dir_name(branch: &str) -> String {
    let mut out = String::new();
    for c in branch.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches(|c| c == '-' || c == '.').to_string();
    if out.is_empty() {
        "worktree".into()
    } else {
        out
    }
}

pub fn checkout_slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.'])
        .to_string()
}

pub fn store_hash(s: &str) -> String {
    format!("{:08x}", crate::util::fnv1a(s.as_bytes()) >> 32)
}

fn repo_name(repo: &Path) -> String {
    repo.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into())
}

pub fn store_dir(worktree_dir: &Path, root: &str) -> PathBuf {
    let name = Path::new(root)
        .file_name()
        .map(|n| task_slug(&n.to_string_lossy()))
        .unwrap_or_else(|| "repo".into());
    worktree_dir.join(format!("{name}-{}", store_hash(root)))
}

pub fn feedback_checkout_dir(worktree_dir: &Path, clone: &Path, branch: &str) -> PathBuf {
    worktree_dir
        .join(checkout_slug(&repo_name(clone)))
        .join(checkout_slug(branch))
}

pub fn patch_checkout_dir(worktree_dir: &Path, repo: &Path, branch: &str) -> PathBuf {
    worktree_dir
        .join(checkout_slug(&repo_name(repo)))
        .join(format!("{}-patch", checkout_slug(branch)))
}

pub const TASK_BRANCH_PREFIX: &str = "task/";
pub const SESSION_BRANCH_PREFIX: &str = "sb/";
pub const REVIEW_BRANCH_PREFIX: &str = "review/pr-";
pub const OWNED_BRANCH_PREFIXES: [&str; 3] = [
    SESSION_BRANCH_PREFIX,
    REVIEW_BRANCH_PREFIX,
    TASK_BRANCH_PREFIX,
];

pub fn review_branch(number: impl std::fmt::Display) -> String {
    format!("{REVIEW_BRANCH_PREFIX}{number}")
}

pub fn task_branch(number: impl std::fmt::Display, title: &str) -> String {
    format!("{TASK_BRANCH_PREFIX}{number}-{}", task_slug(title))
}

pub fn session_branch_prefix(agent: Option<&str>) -> String {
    match agent {
        Some(a) => format!("{SESSION_BRANCH_PREFIX}{}/", task_slug(a)),
        None => SESSION_BRANCH_PREFIX.into(),
    }
}

pub fn patch_session_name(branch: &str) -> String {
    format!("sb-patch-{}", checkout_slug(branch).replace('.', "-"))
}

pub struct ManagedRoot {
    plain: String,
    canonical: Option<String>,
}

impl ManagedRoot {
    pub fn new(worktree_dir: &Path) -> Self {
        Self {
            plain: worktree_dir.to_string_lossy().into_owned(),
            canonical: worktree_dir
                .canonicalize()
                .ok()
                .map(|c| c.to_string_lossy().into_owned()),
        }
    }

    fn roots(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.plain.as_str()).chain(self.canonical.as_deref())
    }

    pub fn holds_entry(&self, path: &str) -> bool {
        self.roots().any(|r| path.starts_with(&format!("{r}/")))
    }

    pub fn holds_checkout(&self, path: &str, canonical_path: &str) -> bool {
        self.roots()
            .any(|r| inside(path, r) || inside(canonical_path, r))
    }
}

pub fn inside(child: &str, dir: &str) -> bool {
    child == dir
        || child
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_keep_their_distinct_outputs() {
        assert_eq!(task_slug("Fix the Login page!"), "fix-the-login-page");
        assert_eq!(task_slug("  ##  "), "task");
        assert_eq!(branch_dir_name("feat/AS-12_x..y"), "feat-AS-12_x..y");
        assert_eq!(branch_dir_name("//"), "worktree");
        assert_eq!(checkout_slug("..a.b_c/"), "a.b_c");
        assert_eq!(checkout_slug("///"), "");
    }

    #[test]
    fn store_hash_is_the_high_half_of_fnv() {
        assert_eq!(store_hash("").len(), 8);
        assert_eq!(
            store_hash("/a"),
            format!("{:08x}", crate::util::fnv1a(b"/a") >> 32)
        );
        assert_ne!(store_hash("/a"), store_hash("/b"));
    }

    #[test]
    fn layouts_join_under_the_worktree_dir() {
        let wd = Path::new("/w");
        assert_eq!(
            feedback_checkout_dir(wd, Path::new("/src/My Repo"), "feat/x.y"),
            Path::new("/w/My-Repo/feat-x.y")
        );
        assert_eq!(
            patch_checkout_dir(wd, Path::new("/src/r"), "feat/x"),
            Path::new("/w/r/feat-x-patch")
        );
        assert_eq!(
            feedback_checkout_dir(wd, Path::new("/"), "b"),
            Path::new("/w/repo/b")
        );
        assert_eq!(
            store_dir(wd, "/src/My Repo"),
            PathBuf::from(format!("/w/my-repo-{}", store_hash("/src/My Repo")))
        );
        assert_eq!(
            store_dir(wd, "/"),
            PathBuf::from(format!("/w/repo-{}", store_hash("/")))
        );
    }

    #[test]
    fn branch_and_session_names() {
        assert_eq!(task_branch(7, "Fix login"), "task/7-fix-login");
        assert_eq!(session_branch_prefix(Some("Claude")), "sb/claude/");
        assert_eq!(session_branch_prefix(None), "sb/");
        assert_eq!(review_branch(12), "review/pr-12");
        assert_eq!(OWNED_BRANCH_PREFIXES, ["sb/", "review/pr-", "task/"]);
        assert_eq!(
            patch_session_name("feature/AS-1.2"),
            "sb-patch-feature-AS-1-2"
        );
    }

    #[test]
    fn managed_root_variants_differ_on_the_root_itself() {
        let m = ManagedRoot::new(Path::new("/nonexistent-root-xyz"));
        assert!(m.holds_entry("/nonexistent-root-xyz/a"));
        assert!(!m.holds_entry("/nonexistent-root-xyz"));
        assert!(m.holds_checkout("/nonexistent-root-xyz", "/elsewhere"));
        assert!(m.holds_checkout("/elsewhere", "/nonexistent-root-xyz/a"));
        assert!(!m.holds_checkout("/nonexistent-root-xyz2/a", "/other"));
    }
}
