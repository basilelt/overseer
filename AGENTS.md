# Overseer: working rules for coding agents

## Non-negotiables for every task

These come before any new feature. A change that breaks one of them is not done.

- **Stability**: no crash, freeze or lost session. Terminals and agents must survive reconnects, sleep, updates and server upgrades. Handle errors explicitly; never let one failure take down the app or the daemon.
- **Energy**: near-zero work when idle. No polling loops, perpetual animations or redraws when nothing changes; pause everything that isn't visible; debounce events.
- **Efficiency**: keep RAM and CPU low on the app (desktop and mobile) and on the daemon. Prefer incremental and event-driven over full rescans; measure when in doubt (the CI perf budget must stay green).
- **No regressions**: every existing feature keeps working. Add or update a test for each behaviour you touch, and run the existing tests around it before pushing.
- **Coverage only goes up**: every change adds or extends tests so coverage rises. The thresholds in `app/vite.config.ts` (`test.coverage.thresholds`) gate `make app-test`; never lower them, and raise them to one point below the new measured value when your tests lift coverage. New modules ship with their tests; new logic in `src/` ships with a Rust test.
- **Performance is tested**: changes on a hot path (event reduction, terminal rendering, derivations, daemon board/scan) keep the perf tests green (`app/src/lib/perf/`, `tests/*perf*.rs`, bundle budget, `scripts/perf-cpu.sh`). Never raise a budget to make a test pass; fix the slowdown. Perf tests assert work (calls, paints, allocations, scaling), not wall-clock time.
- **Local tests first**: run the relevant checks locally (see below) before any push; CI is the final gate, not the first one.

## Tests are part of the feature

A feature or fix is not done until its tests are. The agent that writes the code writes and maintains its tests in the same branch; nobody adds them later.

- **Write**: every new module gets its own test file next to it (`foo.ts` → `foo.test.ts`; `foo.svelte.ts` → `foo.svelte.test.ts` or `foo.store.test.ts` when `foo.test.ts` exists). Every new daemon endpoint or behaviour gets a Rust unit test in its module or an integration test in `tests/`. Cover the happy path, the error paths (daemon down, 404/410, rejected promise, offline) and the edge cases the code branches on.
- **Bug fixes**: start with a test that fails on the bug, then fix it; the test stays.
- **Maintain**: when behaviour changes on purpose, update the tests that pin it in the same commit. Never delete, weaken or skip a failing test to get green; fix the code or rewrite the test to the new intended behaviour. Remove tests only together with the code they cover.
- **Hot paths**: new code on a hot path (event reduction, derivations over all lines/tasks/terms, terminal rendering, markdown/diff/review rows, daemon board/scan/paging) gets a perf test in `app/src/lib/perf/` or `tests/perf_*.rs` that asserts work or linear scaling (1× vs 10× input), not milliseconds.
- **Skipped tests**: an `it.skip` or `#[ignore]` is only allowed as a gate for a known, reported bug, with the reason in its name or attribute, and is enabled in the commit that fixes the bug.
- **Fast and deterministic**: no real sleeps in app tests (`vi.useFakeTimers`, `vi.waitFor`); restore every global, stub and singleton in `afterEach`; a test must pass alone, in the full suite and with `--sequence.shuffle`. Keep each file under ~2 s; the Rust tests may wait only for real daemon timings.
- **Shape**: test behaviour through the public API of the module (a `MockDaemon` for the app, the HTTP daemon for Rust), not private internals; one behaviour per `it` with a name that states it.

## A feature works for the user, not only in tests

Green unit tests and mock screenshots do not prove a feature works. A feature that reaches the user broken is a failure of its author, whatever the tests said.

- **Real daemon for every new route**: each new or changed daemon endpoint gets an integration test in `tests/` that starts the real daemon (`common::Fx` + `fx.daemon(..)` + `fx.req(..)`) and sends the request byte for byte as the app does: same method, path, URL-encoded query (`encodeURIComponent`) and JSON body. Calling the handler with a hand-built `Request` does not count: it skips request parsing, decoding and routing.
- **The mock is not the contract**: `MockDaemon` must never be more lenient than the daemon (decoding, validation, defaults, error codes). When adding a mock route, copy the daemon's behaviour and its error messages, and check both against the same cases.
- **Exercise it end to end before calling it done**: before opening or marking a PR ready, run the feature once against a real local daemon with real data (not `VITE_MOCK`): drive the exact user flow, including the external CLIs or services it calls, and check the result on screen. If something cannot be run locally, say so in the PR and in the report instead of claiming it works.
- **Test the failure the user would see**: for each user-visible error string a new route can return, have a test that sends the input the app really sends and asserts it does not get that error on the happy path.
- **No "done" without proof**: the final report states what was exercised against the real daemon and what was only covered by mocks.

## QA like a human, with real agents

Tests check what their author thought of; a user clicks everything. In v0.23.0 every suite was green while both buttons of the Ctrl+W dialog did nothing, and only clicking them found it. Before a PR is marked ready, and again on any branch that merges several others, use the app the way a person would.

- **Follow the QA path**: [`docs/qa-path.md`](docs/qa-path.md) is the walkthrough to take: setup, then numbered **Do → Expect** steps over the whole app (sessions, tasks, review, terminals, chat, workspaces, ports, settings, feedback, phone, life events). A feature PR runs the smoke steps, the areas it touches and the life events; a branch that merges several others, or a release, runs all of it. The PR lists the step IDs run and the ones that could not be.
- **The real app on a real daemon**: `scripts/qa/sandbox.sh build` then `start` runs a daemon built from the branch in a sandboxed `HOME` and serves the production frontend (not `VITE_MOCK`); `scripts/qa/drive.py` opens it in headless Chromium with `scripts/qa/shim.js` standing in for the Tauri IPC. Never touch the user's own daemon, state or sessions.
- **Every mode**: users run the desktop app, a browser, Android phones and tablets, iPhones and iPads. `scripts/qa/drive.py` runs the frontend in the `web`, `android`, `android-tablet`, `android-tablet-landscape` (Chromium), `iphone`, `ipad` and `ipad-landscape` (WebKit) modes with touch and device sizes, and `scripts/qa/overflow.py` checks every mode for controls pushed off the screen. Run the steps a change touches in each mode they apply to (`docs/qa-path.md`, Setup).
- **The real desktop app too**: Chromium is not what users run. For anything WebKit can treat differently (CSS, text selection, drag and drop, pointer gestures, clipboard, keyboard shortcuts, terminals), also run the flow in the real Tauri app: `scripts/qa/sandbox.sh desktop-build` then `desktop` starts it under Xvfb against the same sandboxed daemon, driven with `xdotool` and checked with `sandbox.sh shot` and `xclip` (setup in `docs/qa-path.md`). The steps marked **(desktop)** there are only done when run in it. Every Chromium run of the kanban drag was clean while, in the desktop app, it selected the text of the whole board.
- **Keep the path current**: a PR that adds, changes or removes something the user can do updates `docs/qa-path.md` in the same PR (rules in its last section), and extends `scripts/qa/shim.js` when the feature needs a Tauri command the shim does not handle yet.
- **Real agents**: the sandbox puts `tests/stubs` first on the daemon's `PATH`, so tasks and sessions start real agent processes in real terminals and worktrees. Start tasks, type into terminals, let agents finish, review and approve them, and read the stub log to check the exact commands the daemon ran.
- **Every control you touched**: click each button, including Cancel and the secondary actions, try each checkbox and keyboard shortcut, trigger every error path the user can reach (agent stopped, daemon down), then reload the app and check the state survived. Look at the result in both themes and on a phone viewport.
- **Life events**: kill the daemon with `kill -9` and restart it, reload or relaunch the app, and check sessions, tabs and workspaces come back as they were. Stability is a non-negotiable, so a feature that breaks after a restart is not done.
- **Merged work**: when several branches are combined, QA the merged tree; a conflict resolved by hand is new code.
- **Fix what QA finds, now**: every bug QA turns up is fixed right away, with a test that fails before the fix. In the same PR when it belongs to that change; otherwise immediately in its own fix PR, QA'd the same way, even when the bug predates the branch. Never leave it as a note, a "follow-up" or an issue for later.
- **Report it**: the PR lists the flows done by hand and the ones that could not be (for example a picker the stub agent cannot draw, or a native clipboard), each with the test that covers it instead, and the bugs QA found with the PR that fixed each.

## Branches, PRs and releases

Two long-lived branches: `next` collects every feature and fix, `main` only moves when a release is cut (docs/ci.md, "Workflow"). One feature or fix = one branch = one PR to `next`:

- Work on a `<type>/<description>` branch from an up-to-date `next` (hotfixes too). Push it only after the local checks below pass, batching fixes into one push.
- Open the PR to `next` as a draft (`gh pr create --draft --base next`) with a Conventional Commits title. CI does not run on drafts.
- Mark it ready when the branch is complete: CI runs once, only the jobs the changed paths need. Further pushes rerun it.
- A PR does not have to be up to date with its base to merge; merge the base into it only on a conflict.
- Merge with a merge commit (not squash, not rebase). Merging into `next` releases nothing.
- A release is a PR from `next` to `main`, opened by the maintainer when enough work has been batched: first land a `chore(release): vX.Y.Z` commit on `next` through its own PR (made with `scripts/set-version.sh X.Y.Z`, X.Y.Z being the version the merge will tag; a later `feat` on `next` means redoing it; the `release-version` job fails otherwise), then open the release PR as a draft, run the full QA path of `docs/qa-path.md` on it (it merges several branches), mark it ready, merge with a merge commit so the release gate sees the tested tree and does not run CI again on `main`. Then sync `next` with a PR from `main` to `next` (same tree, so CI runs only `changes`), merged with a merge commit: otherwise the next release PR is behind `main` and the release gate runs CI again on `main`.
- Never open a PR from a feature branch to `main`, and never push to `main` or `next` directly.
- `main` and `next` are protected by the `main` and `next` rulesets (`.github/rulesets/main.json`, `.github/rulesets/next.json`): no direct push, force push or deletion; a PR with every CI job green (not required to be up to date with the branch); merge commits only. Repository admins can bypass the `next` ruleset when merging a PR ("Merge without waiting for requirements"), for exceptional cases only. On `main` they can bypass it entirely, direct push included, for urgent CI or release fixes only. Neither branch can be deleted by anyone (`.github/rulesets/no-delete.json`, no bypass), since head branches are deleted on merge. Release tags (`v*`, `latest`) cannot be deleted (`.github/rulesets/release-tags.json`); the release workflow still creates and moves them. Change a ruleset by editing its file and applying it with `gh api -X PUT repos/basilelt/overseer/rulesets/<id> --input <file>`.
- Each merge to `main` is a release: the next `vX.Y.Z` tag is computed from the Conventional Commits since the current tag, the whole batch from `next` included (`feat` = minor, `fix` and others = patch, `!` = major), then the builds (they stamp the version again, a no-op once the release PR carries it), the Android APK (Obtainium), the GitHub Release and the Homebrew tap follow. Docs-only merges make no release.
- The App Store and Play Store are never released automatically: the maintainer runs Actions > Store (`.github/workflows/store.yml`, `workflow_dispatch` from `main`) for the release tag to ship. Store submissions (`upload_to_app_store`, `submit_for_review`, `upload_to_play_store`, ...) live only in `store.yml` and `app/fastlane/`; `scripts/store-guard.sh` fails CI otherwise.

## Local checks before pushing

Run the checks for what changed, gently (`nice -n 10`, `-j 2`):

- app: `pnpm exec vitest run <touched files>`, `pnpm exec svelte-check`; before pushing, `pnpm test:coverage` (full suite plus coverage gate; `pnpm test:fast` skips the production-build bundle budget while iterating)
- Rust: `cargo test --offline -j 2 <tests>`, `cargo clippy -j 2 --all-targets -- -D warnings`, `cargo fmt --check` on the changed crates

Keep Docker, the full `make test` and release builds in CI.

## Tour of the update

Every minor or major release (`x.y.0` or `x.0.0`) ships a tour of what it adds; patch releases (`x.y.z`) do not touch the tour. Any `feat` (or `!`) PR to `next` with a user-visible feature carries its own tour of the update:

- Add one step per new feature to `STEPS` in `app/src/lib/tour/steps.ts`, with `since` set to the release it ships in as `'x.y'`: the latest `v*` tag with the minor bumped (or the major for a `!` change), which every `feat` batched on `next` for the same release shares. Check it again on the release PR from `next` to `main`, since a `!` in the batch or a release cut meanwhile changes that number. Put its `data-tour` target on the real control. `TOUR_VERSION` follows from the newest `since`.
- Write its `tour.<id>.title` and `tour.<id>.body` (plus `tour.<id>.phone` when the phone layout differs) in `app/src/lib/i18n/en.ts`, as in "UI strings" below.
- The app records the newest release whose tour the user saw (`tourSeen`). After an update it shows every step newer than that under **What's new**, oldest release first, each labelled "New in x.y": a user going from 0.22 to 0.26 sees the steps of 0.23 to 0.26 in one tour. New users get every step in the full tour.
- Extend `app/src/lib/tour/steps.test.ts` to pin that `stepsFor('whatsNew', '<previous release>')` returns exactly the new steps, and screenshot the step (desktop and phone) as in the visual check below.
- Keep `docs/user-guide.md` (14.0) in line with the step list.

## Visual check for UI changes

For any change to what the app shows, build the frontend in mock mode (`VITE_MOCK=1 pnpm exec vite build`), serve it with `pnpm exec vite preview`, and take headless Chromium screenshots of the touched views (desktop 1440×900 light and dark, phone 390×844 with `?phone=1`; `?theme=light|dark`). Look at every screenshot and fix overlaps, overflow, clipped text and unreadable contrast before pushing. Stop the preview server and the browser afterwards.

## UI strings

- Write every user-visible string in English only, in `app/src/lib/i18n/en.ts` (or the module's namespace there, such as `usage.*`), and read it with `t()` or the module helper. Never hand-write other languages.
- Other languages live in `app/src/lib/i18n/<locale>.ts` and may lag behind: a missing key shows in English. `pnpm i18n` (in `app/`) translates the missing keys of every locale with `claude -p`; `pnpm i18n --dry` only counts them; `pnpm i18n <locale>` adds a new language, which then appears in Settings without code changes.

## Keep the README current

`README.md` is the first page a new user sees; it must describe the product as it ships, in the same PR as the change.

- **New or removed user-facing feature**: add, update or remove its entry under "Features" (a short paragraph in plain words for a user, not an API reference; details go to `docs/user-guide.md`).
- **Changed view**: when a view that has a screenshot in `app/docs/screenshots/` changes visibly, retake that screenshot from the mock build (desktop 1440×900, same theme as the existing file) and commit it with the change.
- **Tour**: when a step of the guided tour (`app/src/lib/tour/steps.ts`) or the window it walks through changes, regenerate `docs/media/tour.gif` with `scripts/readme-tour.sh`.
- **Install, upgrade or supported agents and platforms change**: update the matching README section.
- Keep media small (GIF under ~1 MB, PNG screenshots from the mock, no real names, tokens or private data) and link to files in the repository with relative paths.

## Git

- Conventional commits; branch names `<type>/<description>`.
- Feature and fix PRs target `next`; only release PRs target `main`. Every PR is opened as a draft and marked ready when the branch is complete.
- The user is the only author: no Co-Authored-By or any Claude/session attribution in commits, PRs or comments.
