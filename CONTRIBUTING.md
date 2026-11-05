# Contributing to Overseer

Thanks for helping. This page explains how to propose a change and what a change needs before it
can be merged. By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

- **Bugs and ideas**: search the [issues](https://github.com/basilelt/overseer/issues) first, then
  open one with the matching template. For anything bigger than a small fix, wait for a reply on
  the issue before writing code, so the work is not wasted.
- **Security problems**: never open a public issue. Follow [SECURITY.md](SECURITY.md).
- **Questions**: open an issue and say it is a question.

## The rules every change follows

[AGENTS.md](AGENTS.md) is the full rulebook. It is written for coding agents, and it applies to
people too. The short version:

- **Stability**: no crash, freeze or lost session. Terminals and agents survive reconnects, sleep,
  updates and daemon restarts.
- **Energy and efficiency**: near-zero work when idle; no polling loops or animations that run
  when nothing changes; keep RAM and CPU low on the app and the daemon.
- **No regressions, tests included**: every behaviour you add or change ships with its tests in
  the same PR. Bug fixes start with a test that fails on the bug. Coverage only goes up: never
  lower the thresholds in `app/vite.config.ts`.
- **Performance is tested**: changes on a hot path keep the perf tests green. Never raise a
  budget to make a test pass.
- **Real daemon**: a new or changed daemon endpoint gets an integration test in `tests/` that
  starts the real daemon. The app's `MockDaemon` must never be more lenient than the daemon.
- **QA like a user**: for user-visible changes, walk the matching steps of
  [docs/qa-path.md](docs/qa-path.md) against a sandboxed daemon and list them in the PR.
- **Docs move with the code**: update `README.md`, `docs/user-guide.md` and `docs/qa-path.md`
  when what the user can do changes. A `feat` with a visible feature adds a tour step
  (`app/src/lib/tour/steps.ts`).
- **UI strings**: English only, in `app/src/lib/i18n/en.ts`. Do not hand-write other languages.
- **No new dependency** for something a few lines can do.

## Set up

You need Rust (stable), Node.js with pnpm, git, tmux and, on Linux, the Tauri system libraries
(`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`).

```sh
git clone https://github.com/<you>/overseer && cd overseer
make test                      # daemon tests, offline, isolated tmux server
cd app && pnpm install
pnpm exec vitest run           # app tests
```

[app/README.md](app/README.md) covers the app's development loop and the mock daemon.

## Make the change

1. Fork the repository and create a branch from an up-to-date `main`, named
   `<type>/<description>` in lowercase with hyphens, for example `fix/held-session-resume` or
   `feat/task-filters`. Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`, `ci`,
   `build`, `style`, `revert`.
2. Write [Conventional Commits](https://www.conventionalcommits.org/) messages
   (`fix(kanban): keep the toolbar on narrow screens`). The PR title follows the same format: each
   merge to `main` is a release, and its version comes from these titles (`feat` = minor,
   `fix` and others = patch, `!` = major).
3. Sign off every commit (`git commit -s`). The sign-off certifies the
   [Developer Certificate of Origin](https://developercertificate.org/): you wrote the change, or
   have the right to submit it under the project's license.
4. Run the checks for what you changed, then push:

   | Changed | Run |
   | --- | --- |
   | app (`app/src`) | `pnpm exec vitest run <files>`, `pnpm exec svelte-check`, then `pnpm test:coverage` before pushing |
   | daemon or API (Rust) | `cargo test <tests>`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` in each changed crate |
   | UI | screenshots of the mock build (`VITE_MOCK=1 pnpm exec vite build`), desktop light and dark plus phone width |

5. Open the pull request as a **draft** and fill in the template. Mark it ready when the branch
   is complete: CI runs then. A maintainer approves CI runs for first-time contributors.
6. A maintainer reviews it and merges it with a merge commit once CI is green and the branch is
   up to date with `main`.

Changes written with AI coding agents are welcome. You are responsible for them like any other
code: read the diff, run the checks, and answer review comments yourself.

## License

Overseer is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE). By
submitting a contribution you agree that it is licensed under the same terms.
