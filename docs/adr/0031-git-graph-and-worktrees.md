# 0031 Worktree-first sessions and a per-project git graph

Status: accepted

## Context
Tasks already ran in worktrees (`<worktreeDir>/<repo>/task-<n>-<slug>` from `HEAD`), but agent
sessions started from the app ran in whatever checkout the user picked, so two agents could
edit the same tree. Worktrees piled up: nothing told a finished one from one still holding
work, the default store `~/worktrees` mixed task, runner and hand-made worktrees, and
`<repo>/.overseer/worktrees` would put them inside the repository (in `git status`, in
editors' file watchers, in builds). Users also asked for a git graph per project that shows
where their agents are.

## Decision
- **One store, outside every repository**: `<worktreeDir>/<repo-name>-<hash>/<branch with / as ->`,
  `worktreeDir` defaulting to `~/.overseer/worktrees` (was `~/worktrees`); `<hash>` = 8 hex
  digits of FNV-1a 64 over the main worktree root, so two clones named `app` never share a
  folder. `src/worktrees.rs` owns creation, listing, cleanup, merge-base and cherry-pick; the
  task start path calls it (same branch names, restart reuses the worktree). The runner's
  `start`/`patch --local` keep their own layout under the same `worktreeDir`.
- **Naming**: `sb/<agent>/<slug>` for sessions (`MMDD-HHMM` by default), `task/<n>-<slug>` for
  tasks, `sb/<slug>` for a worktree made from the graph; a taken name gets `-2`, `-3`.
- **Base** = the default branch (`origin/HEAD`, `main`, `master`, else the checkout's branch),
  fetched first (`worktreeFetch`, 10 s timeout, failures ignored). Start from the local branch
  when it contains `origin/<default>`, else from `origin/<default>`: local unpushed work on the
  default branch is kept and a stale local branch is not used. Previously tasks started from
  `HEAD` of the main checkout, i.e. from whatever branch the user had out.
- **Default on, opt-out per start**: `POST /v1/terms {worktree}` and task start `{inPlace}`. The
  daemon keeps the old behaviour without these fields; the app sends `worktree: true` unless
  the user picks "Run in current checkout".
- **Managed vs orphan** by location (under `worktreeDir`) or reference (a task's
  `worktreePath`), no new table: other worktrees of the repository are listed as orphans and no
  action ever touches them.
- **Lifecycle** is computed, not stored: `missing`, `in_use` (alive line, running term or
  in-progress task inside), `stale` (clean, and merged — tip reachable from the base after the
  branch moved since its creation per its reflog, or PR `MERGED` in the gh cache — or idle for
  `worktreeStaleDays`), else `idle`.
- **Cleanup cannot lose work**: dry run by default, the plan lists every removal; refused for
  the main checkout, orphans, locked, in-use, dirty (untracked included, ignored excluded), or
  branches with commits in neither the base nor a remote. `git worktree remove` is never forced;
  the branch is deleted only when all its commits are in the base or it was merged through a
  PR whose tip is pushed. A missing directory loses only its own administrative entry (not a
  repository-wide `git worktree prune`, which would also prune orphans).
- **Graph**: the daemon sends commits (sha, parents, author, time, subject) in `--date-order`,
  paged by an opaque offset cursor, plus all refs and Overseer overlays (lines, tasks, managed
  worktrees, PR from the gh cache only, filled in the background). The lane layout runs in the
  app as a pure, incremental function: a page is laid out from the previous page's open lanes,
  so paging never recomputes or resends earlier rows, the payload is about ten times smaller
  than per-row segments, and the mock daemon uses the same code. The layout's fixture tests
  (linear, merges, octopus, crossing, missing parents, page boundaries) live with it in vitest;
  the daemon side is tested on real temporary repositories with the same shapes.
- **Live refresh without a dependency**: no `notify` crate (offline builds). A thread fingerprints
  `HEAD`, `packed-refs`, `refs/**` and `worktrees/*/HEAD` (path, size, mtime) each second, only
  for projects whose graph or worktrees were requested in the last 10 minutes, and publishes
  `git.changed`. PR lookups and directory sizes never block a request: they are cached and
  announced with `git.changed` / `worktree.updated` when ready.

## Consequences
- Existing task worktrees under `~/worktrees` keep working (a task reuses its recorded path) and
  are listed as managed; new ones go to the new store. Set `worktreeDir` to keep the old place.
- Projects and worktrees are addressed by their percent-encoded absolute path, so the routes stay
  stateless and work on a remote daemon unchanged. A `missing` worktree needs the project in the
  cleanup body to be found.
- All additions are additive; the protocol stays 1.
