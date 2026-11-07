# CI and releases

Six workflows in `.github/workflows/` (ADR 0027; mobile builds ADR 0032 and
[docs/mobile.md](mobile.md)). Every job runs on a GitHub-hosted runner; the repository needs no
self-hosted runner.

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | non-draft pull requests (`opened`, `synchronize`, `reopened`, `ready_for_review`), manual (`workflow_dispatch`), and called by `release.yml` | lint, tests, Windows check, debug APK |
| `release.yml` | push to `main` | CI, version tag, binaries, app bundles, Android APK, GitHub Release, Homebrew tap |
| `store.yml` | manual, from `main` only | App Store submission (iOS) and Play Store upload (Android) of a release tag |
| `release-pr.yml` | manual | opens the draft release PR from `next` to `main` (skips when one is open or `next` is not ahead) |
| `mobile-init.yml` | manual | generates `app/src-tauri/gen/{android,apple}` and commits them to the branch |
| `pages.yml` | push to `next` that changes `docs/showreel/` or this workflow, manual | deploys `docs/showreel/` (the showreel page, the repository's website) to GitHub Pages |

Concurrency: `ci-pr-<PR number or ref>` cancels the previous run of the same PR;
`release-refs/heads/main` never cancels a running release (a newer push waits); `store` never
cancels a running store upload; `pages` never cancels a running deployment.

Runner budget: CI runs nothing on draft PRs, only the jobs the change needs on ready PRs
([Path filtering](#path-filtering)), and is not repeated on `main` when the merged PR already passed
it ([Release gate](#release-gate)). Features and fixes collect on the `next` integration branch
and reach `main` in batches, so a release (and its full build) happens only when the maintainer
decides. See [Workflow](#workflow) for how branches reach `main`.

A release run with no secret configured goes green: it publishes the daemon binaries and the
desktop bundles, unsigned or ad-hoc signed, and skips with a warning everything that needs a
secret ([Secrets](#secrets)).

## Workflow

Two long-lived branches: `next`, the integration branch (the default branch on GitHub, so pull
requests target it unless told otherwise), and `main`, which only moves when a release is cut.
Every push to `main` is a release; `next` collects the work in between.

One feature or fix = one branch = one PR to `next`:

1. Branch `<type>/<description>` from an up-to-date `next`. Push once the local checks of
   `AGENTS.md` pass, batching fixes into one push.
2. Open a draft PR to `next` with a Conventional Commits title: no CI runs while it is a draft.
3. Mark it ready when the branch is complete, so CI runs once. Further pushes rerun it (the
   previous run is cancelled).
4. Merge with a merge commit (not squash, not rebase), after merging `next` into the branch if
   `next` moved and CI is green again. Merging into `next` releases nothing.

Hotfixes take the same path: a `fix/` branch from `next`, a PR to `next`, then a release PR.

Cutting a release, when enough has been batched on `next`:

1. Stamp the version first: `next` is protected, so the `chore(release): vX.Y.Z` commit (made with
   `scripts/set-version.sh X.Y.Z`, `X.Y.Z` being the version the merge will tag) reaches it through
   its own PR to `next` ([Versioning](#versioning)). A `feat` merged on `next` after that PR
   changes the expected version: redo the version PR.
2. Open a draft PR from `next` to `main` (title such as `chore: release`), by hand or with
   Actions > Release PR (`release-pr.yml`), run the full QA path of
   [docs/qa-path.md](qa-path.md) on the merged tree, then mark it ready: CI runs once on the whole
   batch.
3. Merge with a merge commit. The release then skips CI (the gate sees the same tree already
   passed). `tag` bumps the current `vX.Y.Z` tag from every Conventional Commit of the batch
   ([Versioning](#versioning)): one `feat` anywhere in it makes a minor release. Then the builds,
   the Android APK (Obtainium), the GitHub Release and the Homebrew tap follow. A docs-only merge
   makes no release.
4. `main` now has a merge commit that `next` lacks. Sync right away with a PR from `main` to
   `next`, merged with a merge commit: the trees are equal, so CI runs only `changes`. Without the
   sync the next release PR is behind `main`, and the release gate runs CI again on `main` before
   tagging.

Store releases are separate and manual: Actions > Store > Run workflow, on `main`
([Store releases](#store-releases)).

Actions > CI > Run workflow runs every job of `ci.yml` on any branch, `macos` included, without a
pull request.

## Path filtering

The `changes` job (`ubuntu-slim`, checkout with `fetch-depth: 2`) runs `scripts/ci-changes.sh`,
which lists `git diff --name-only HEAD^1 HEAD` of the PR merge ref (the PR's changes against its
base) and sets one output per job. Each job `needs: changes` and runs only when its output is
`true`:

| Job | Runs when the PR changes |
|---|---|
| `rust` | `src/`, `api/`, `tests/`, `board/` (embedded in the binary), `contract/` (schemas used by the tests), `Cargo.toml`, `Cargo.lock`, `build.rs` |
| `app` | `app/` except `app/src-tauri/gen/` and `app/fastlane/`, `contract/`, and everything that triggers `rust` (`make app-test` builds the daemon and runs the Tauri tests against it) |
| `perf` | same as `app`, plus `scripts/perf-cpu.sh` |
| `windows` | `src/`, `api/` (path dependency), `tests/`, `board/`, `Cargo.toml`, `Cargo.lock`, `build.rs` (it checks the root crate only, not `app/src-tauri`) |
| `android` | `app/src-tauri/` (including `gen/android/`), `app/package.json`, `app/pnpm-lock.yaml`, `scripts/android-*.sh` |
| `macos` (signing steps) | `.github/workflows/release.yml`, `scripts/macos-sign.sh`, `app/src-tauri/tauri*.conf.json`, `app/src-tauri/Info.plist` |
| `macos` (`procs`/`ports`/`review::watch` tests) | `src/procs.rs`, `src/ports.rs`, `src/review/watch.rs`, `Cargo.toml` |
| `actionlint` | `.github/` |
| `store-guard` | `.github/`, `scripts/`, `Makefile` (what `scripts/store-guard.sh` scans), `app/fastlane/` and the iOS app icons |
| `tap` | `scripts/update-tap.sh`, `scripts/update-tap.test.sh` |
| `ci-scripts` | `.github/actions/`, `scripts/ci-*.sh` (runs `scripts/check-version*.sh`, `scripts/set-version.sh`; runs `scripts/ci-changes.test.sh`, `scripts/check-version.test.sh`, `scripts/ci-apt.test.sh` and the setup action with apt packages) |

- `docs/`, `*.md` and `.github/ISSUE_TEMPLATE/` never trigger a job: a docs-only PR runs `changes`
  alone.
- `.github/workflows/ci.yml`, `.github/actions/`, `scripts/ci-*.sh` and `Makefile`
  trigger every job except `macos`, which only follows its own paths: macOS runners are the
  scarcest and slowest to start, so it runs on pull requests and manual runs only, as one job.
- A failed `git diff` runs every job.
- A push to `next` (a merged PR) is filtered the same way on `HEAD^1..HEAD`, the merged PR's
  changes, and saves the caches of the jobs it runs ([Caches](#caches)).
- Called from `release.yml` (`workflow_call`), every job runs except `macos`, unless the gate
  passes `skip: true`. A manual run runs every job.
- A draft PR skips `changes`, so every job is skipped; `ready_for_review` starts the real run.

The `main` ruleset counts a job skipped by its `if` as passed, so a docs-only PR can merge. Draft
PRs cannot be merged anyway.

## Release gate

The first job of `release.yml`, `gate` (`ubuntu-slim`, full history, `checks: read`,
`pull-requests: read`), runs `scripts/release-gate.sh` and sets two outputs:

- `release=false` when every file changed by the push (`git diff <before> HEAD`) is under `docs/`,
  `.github/ISSUE_TEMPLATE/` or a `*.md`: `ci`, `tag` and every build are skipped, no tag. Those
  commits are part of the next release's changelog.
- `release=false` also when `HEAD` already carries a `vX.Y.Z` tag (step "Commit already
  released"): a commit pushed together with its release tag, as the first push of the repository
  is, builds nothing.
- `skip_ci=true` when all of these hold:
  1. `HEAD` is a two-parent merge commit;
  2. `git merge-base --is-ancestor HEAD^1 HEAD^2`: the PR branch contained `main`;
  3. `git diff --quiet HEAD^2 HEAD`: the merged tree is the tested tree;
  4. `HEAD^2` is the head of a PR (`commits/<sha>/pulls`);
  5. every latest check run on `HEAD^2` is completed with `success`, `skipped` or `neutral`, and
     the `changes` check run succeeded (a draft run skips it, so this proves a non-draft PR run).

  Otherwise (squash or rebase merge, branch behind `main`, a direct push, a red or pending check,
  an API error) CI runs in the release run.

`ci` is never skipped itself: a skipped job also skips every later job whose `if` has no status
function, which would skip the builds. With `skip_ci=true` it is called with `skip: true` and only
its `changes` job runs, a few seconds on `ubuntu-slim`. Pushes by `github-actions[bot]` or with
`[skip ci]` in the head commit message skip `gate`, hence the whole release.

## Jobs and runners

| Workflow | Job | Runner | What |
|---|---|---|---|
| ci | `changes` | `ubuntu-slim` | `scripts/ci-changes.sh`: which jobs the PR needs ([Path filtering](#path-filtering)) |
| ci | `actionlint` | `ubuntu-slim` | `actionlint` (pinned release, checksum verified, cached by version; `ubuntu-slim` has no Docker) |
| ci | `ci-scripts` | `ubuntu-24.04` | `scripts/ci-changes.test.sh` (path filter), `scripts/ci-glibc.test.sh` (glibc floor check), `scripts/check-version.test.sh`, `scripts/ci-apt.test.sh` (offline apt install in Docker), the setup action on real apt packages |
| ci | `release-version` | `ubuntu-slim` | pull requests to `main` only, skipped when docs-only: `mathieudutour/github-tag-action@v7` with the settings of the `tag` job of `release.yml` and `dry_run: true` computes the version the merge will tag, then `scripts/check-version.sh` fails naming each stamped file that differs. Required on `main` once `.github/rulesets/main.json` is applied (a skipped job counts as passed). Copy the settings by hand when you change them in `release.yml` |
| ci | `gitleaks` | `ubuntu-slim` | `gitleaks git` (pinned release, checksum verified) over the PR's commits (`base..head`); every PR, no path filter. Known false positives are fingerprinted in `.gitleaksignore` |
| ci | `rust` | `ubuntu-24.04` | `cargo fmt --check` (root and `api/`), `make lint`, `timeout 1500 make test` |
| ci | `app` | `ubuntu-24.04` | `make app-test` (vitest with coverage, svelte-check, Tauri crate tests and clippy) |
| ci | `perf` | `ubuntu-24.04` | release build of the app against the mock backend, `scripts/perf-cpu.sh` CPU budget (30 s idle, 30 s streaming) under Xvfb |
| ci | `windows` | `windows-latest` | native `cargo clippy --target x86_64-pc-windows-msvc --lib --bins -- -D warnings` |
| ci | `android` | `ubuntu-24.04` | debug APK for arm64 (`pnpm tauri android build --debug --apk --target aarch64`), 16 KB page alignment (`scripts/android-16k.sh` and its test), artifact `android-debug-apk` |
| ci | `macos` | `macos-26` | throwaway self-signed identity on two builds must give one designated requirement; `cargo test --lib -- procs:: ports:: review::watch::`; pull requests and manual runs only |
| ci | `store-guard` | `ubuntu-slim` | `scripts/store-guard.sh`: App Store / Play Store submission only in `store.yml`, `store.yml` only runs on `workflow_dispatch`, opaque iOS icons; then `scripts/store-guard.test.sh` |
| ci | `tap` | `ubuntu-24.04` | `scripts/update-tap.test.sh`: runs `scripts/update-tap.sh` on a fake release and requires `brew style` and `brew audit --strict` to pass on the generated formula and cask |
| release | `gate` | `ubuntu-slim` | `scripts/release-gate.sh` and the tagged-`HEAD` check ([Release gate](#release-gate)); skipped for `github-actions[bot]` and `[skip ci]` |
| release | `ci` | calls `ci.yml` | the jobs above, or `changes` alone when the gate says `skip_ci` |
| release | `tag` | `ubuntu-slim` | next `vX.Y.Z` tag, moves `vX`, `vX.Y`, `latest`; checks which optional secrets are set |
| release | `servers` | `ubuntu-24.04` | `make dist-cross`: daemon for Linux musl x86_64/aarch64 and Windows x86_64 (MSVC), `overseer-api` Linux musl x86_64 |
| release | `servers-macos` | `macos-26` | `make dist-darwin`: daemon for macOS x86_64/aarch64, built natively so it links FSEvents (CoreServices needs the Apple SDK) |
| release | `app-linux` | `ubuntu-22.04`, `ubuntu-22.04-arm` | `.deb`, `.AppImage`, `.rpm` for x86_64 and aarch64, each built natively on the oldest supported Ubuntu so they run on glibc 2.35; `scripts/ci-glibc.sh` fails the job if any bundled ELF needs a newer glibc. GitHub retires these runners on 2027-04-17 (actions/runner-images#14254): move the build into a `ubuntu:22.04` container on `ubuntu-24.04` before then |
| release | `app-windows` | `ubuntu-24.04` | NSIS installer `*_x64-setup.exe`, cross-built with `cargo-xwin` |
| release | `app-windows-msix` | `windows-latest` | `Overseer_<v>_x64.msix` from the `app-windows` executable (`scripts/msix.sh`); skipped without the MSIX secrets |
| release | `app-windows-arm` | `windows-11-arm` | NSIS installer `*_arm64-setup.exe` and `Overseer_<v>_arm64.msix`, built natively |
| release | `app-macos` | `macos-26` | `.dmg` for Apple Silicon (`aarch64-apple-darwin`) and Intel (`x86_64-apple-darwin`, cross-built) |
| release | `app-android` | `ubuntu-24.04` | signed universal APK (arm64-v8a, armeabi-v7a) `Overseer_<v>_android.apk` + `.sha256`, for Obtainium and the GitHub Release (the publish job adds a copy named `Overseer_android.apk` for the README's fixed `releases/latest/download/` link); skipped without the keystore secrets |
| release | `publish` | `ubuntu-slim` | GitHub Release with generated release notes, every artifact and `SHA256SUMS`, then the updater manifest `latest.json` when the updater key is set |
| release | `tap` | `ubuntu-24.04` | `scripts/update-tap.sh` writes the cask and formula of `basilelt/homebrew-tap` and pushes `overseer <version>` to `bump/overseer-<version>`; the tap's own CI audits it and fast-forwards `main`; skipped without `HOMEBREW_TAP_DEPLOY_KEY` |
| store | `ios` | `macos-26` | fastlane `app_store` lane: `tauri ios build` signed from `IOS_CERTIFICATE` and `IOS_MOBILE_PROVISION`, `upload_to_app_store` with `submit_for_review` (manual release after approval) |
| store | `android` | `ubuntu-24.04` | signed Google Play App Bundle (`pnpm tauri android build --aab` with `OVERSEER_DISTRIBUTION=play`, checked by `scripts/android-aab.sh` and `scripts/android-16k.sh`), uploaded with `r0adkll/upload-google-play` to the chosen track, status `completed` |

The app jobs get the server binaries from the `servers*` artifacts and copy them into
`app/src-tauri/resources/server/<os>-<arch>/overseer` (`scripts/app-resources.sh`, also `make app-resources`), so every app
bundle can install the daemon on Linux and macOS hosts in remote mode.

The x64 Windows installer stays cross-built on Linux: `servers` needs `cargo-xwin` on Linux anyway
for `overseer.exe` (`make dist-cross`). The arm64 installer builds natively on `windows-11-arm`
in Git Bash, which has no `make`, so the job calls `scripts/app-resources.sh` directly. Every
desktop app job must succeed for `publish` to run.

## Runner setup

Hosted runners have none of the project's tools. Every job installs them through one composite
action, [`.github/actions/setup`](../.github/actions/setup/action.yml), whose inputs say what the
job needs:

| Input | What it does | Jobs |
|---|---|---|
| `free-disk: all` / `keep-android` | deletes `/usr/share/dotnet`, `/opt/ghc`, `/usr/local/.ghcup`, CodeQL (and the Android SDK unless `keep-android`) in the background, at idle I/O priority, while the job goes on | app, perf, android, app-linux, app-android |
| `apt: <packages>` | `scripts/ci-apt.sh`: installs from a cached `.deb` archive (see below), Linux only | rust, app, perf, ci-scripts, servers, app-linux, app-windows |
| `rust: "true"`, `rust-components`, `rust-targets` | `dtolnay/rust-toolchain@stable`, then `Swatinem/rust-cache@v2` on `rust-workspaces` (`rust-cache-key` for matrix variants) | every Rust job |
| `node: "true"` | `pnpm/action-setup@v6` (pnpm 11), `actions/setup-node@v7` (Node 24), the pnpm store cache, then `pnpm --dir app install --frozen-lockfile --prefer-offline` | app, perf, android, app-* |
| `java: "true"` | `actions/setup-java@v6`, Temurin 21 | android, app-android |
| `gradle: "true"` | caches `~/.gradle/caches` and `~/.gradle/wrapper` | android, app-android |
| `xwin: "true"` | caches the MSVC CRT and Windows SDK of `cargo-xwin` (`~/.cache/cargo-xwin`) | servers, app-windows |
| `check: <list>` | `scripts/ci-runner-check.sh` | most jobs |

The Tauri Linux libraries are `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libxdo-dev libssl-dev pkg-config
patchelf` (plus `tmux` for the daemon's terminal tests, `xvfb dbus-x11` for perf, `file` for
app-linux). The cross builds add `mlugg/setup-zig@v2` and `taiki-e/install-action@v2` with
`cargo-zigbuild,cargo-xwin`; The Android SDK and NDK are preinstalled on `ubuntu-24.04` (`ANDROID_HOME`, platform
36, build-tools); `scripts/android-env.sh` runs with `NDK_HOME=$ANDROID_NDK_LATEST_HOME` (the
newest NDK on the image, r28 or later, whose linker aligns native libraries to 16 KB pages for
Google Play; `$ANDROID_NDK_HOME` when unset) and adds the Rust Android targets.
`mobile-init.yml` runs once per platform and `store.yml` checks out the release tag, which may predate the setup action, so both keep their own setup.

`git`, `ssh`, `ssh-keygen`, `curl`, `jq`, `gh` and Docker come with the images. The daemon's
Docker remote tests run only with `OVERSEER_DOCKER_TESTS=1` (`make test-remote`), not in CI.

`scripts/ci-runner-check.sh` fails in a second with the list of what is missing (commands,
`pkg-config` modules, rustup targets) instead of deep in a build, and puts `~/.cargo/bin` on the
`PATH` of the next steps.

### Caches

The repository has 10 GB of Actions cache, evicted least recently used first. A cache saved on a
pull request belongs to that pull request only, and every pull request used to save its own copy
of the same 554 MB Gradle cache. So **pull requests only restore caches**; they are saved by the
CI run on each push to `next` (the default branch, whose caches every pull request can read), by
the release jobs on `main`, and by manual runs (to their own branch).

| Cache | Key | Saved by |
|---|---|---|
| Rust: `~/.cargo` registry and git sources, and the `target` directories (dependencies only) | `Swatinem/rust-cache`: job, rustc version, `Cargo.lock` and `Cargo.toml` files | every Rust job |
| apt `.deb` archive | OS, arch, sorted package list, then the runner image version (`$ImageOS-$ImageVersion`): a new image starts cold, since its lists differ | jobs with `apt` |
| pnpm store | OS and `app/pnpm-lock.yaml`, falls back to any older store | jobs with `node` |
| Gradle | the Android `*.gradle.kts` and wrapper properties, falls back to any older one | android, app-android |
| cargo-xwin | fixed (`cargo-xwin-x86_64-v1`) | servers, app-windows |
| actionlint binary | its version | actionlint |

`scripts/ci-apt.sh` installs without `apt-get update` from the restored archive
(`Dir::Cache::Archives`) and the package lists saved with it (so the lists want exactly the
`.deb` files the archive holds), with the man-db trigger off and `dpkg
--force-unsafe-io`. A warm run downloads nothing. When the archive is missing a `.deb` (cold cache,
new package) the install fails, the script runs `apt-get update` and installs again, and the next `main` run
saves the new archive. `scripts/ci-apt.test.sh` proves it in Docker: a cold install fills the
archive, a second install succeeds with `--network none`, and a missing `.deb` falls back to the
network.

`CARGO_INCREMENTAL=0` everywhere. Standard hosted Linux runners of a public repository have 4
vCPUs, 16 GB of RAM and about 14 GB of guaranteed free disk; the Tauri jobs delete preinstalled
toolchains they do not use (`free-disk`), in the background, since a foreground `rm` took 60 to
240 s per job.

To see what the cache holds: `gh api repos/basilelt/overseer/actions/cache/usage` and
`gh cache list --sort size_in_bytes`; delete stale pull request caches with
`gh cache delete <key> --ref refs/pull/<n>/merge`.

### Perf budget on hosted runners

`scripts/perf-cpu.sh` measures the CPU of the app's process tree as a percentage of one core
(idle budget 3 %, streaming budget 80 %). The budgets were calibrated on 8-core runners; hosted
runners have 4 vCPUs, and WebKit renders in software under Xvfb. The work per frame does not
depend on the core count, but a slower core spends a larger share of its time on the same work,
so the streaming budget may be tight there. The budgets are not raised to make the job pass: if
`perf` fails on hosted runners only, profile the slowdown first.

## Actions

Pinned to a commit SHA with the tag in a comment; Dependabot bumps them monthly (`.github/dependabot.yml`, which also scans `.github/actions/*`).

`actions/checkout@v7`, `actions/setup-node@v7`, `actions/setup-java@v6`, `actions/cache@v6`,
`maxim-lobanov/setup-xcode@v1`, `actions/upload-artifact@v7`, `actions/download-artifact@v8`,
`pnpm/action-setup@v6`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`,
`taiki-e/install-action@v2`, `mlugg/setup-zig@v2`, `mathieudutour/github-tag-action@v7`,
`ncipollo/release-action@v1`.

`tauri-apps/tauri-action` is not used: the bundles come from `pnpm tauri build` and are attached
by the `publish` job, so no job creates releases on its own.

## Versioning

On every push to `main` that is not docs-only and not already tagged (after the gate, and CI when
it runs):

1. `mathieudutour/github-tag-action@v7` reads the Conventional Commits since the last `vX.Y.Z`
   tag: `feat` bumps the minor, `fix`/`chore`/`docs`/others the patch (`default_bump: patch`),
   `BREAKING CHANGE` or `!` the major. It pushes `vX.Y.Z`, and with `rolling_tags` moves `vX`
   and `vX.Y`; the next step force-moves `latest`. With no commit since the last tag it logs
   "No eligible commits" and sets no tag, so every build is skipped and the run stays green.
2. First tag: the repository starts at `v0.26.3`, pushed with the first commit in one push
   (`git push --atomic origin main v0.26.3`), so the gate sees a tagged `HEAD` and the run builds
   nothing. Push the tag with the commit (or before it), not after: a release run that finds no
   `vX.Y.Z` tag at all seeds one from the root `Cargo.toml` version (`custom_tag`) and publishes
   that release. The first merge after that makes `v0.26.4` (or `v0.27.0`).
3. The versions in git tell the truth. Before the release PR is opened, a PR to `next` (`next` is
   protected) adds a `chore(release): vX.Y.Z` commit made with `scripts/set-version.sh X.Y.Z`,
   which writes the version into `Cargo.toml`, `api/Cargo.toml`, `app/src-tauri/Cargo.toml`,
   their `Cargo.lock` entries, `app/package.json` and `app/src-tauri/tauri.conf.json`. `X.Y.Z` is
   the version the merge will tag. A `feat` merged on `next` after that PR changes the expected
   version, so the version PR has to be redone. The `release-version` job checks it on the
   release PR ([Jobs and runners](#jobs-and-runners)). Its check is a test-merge that GitHub does
   not redo when `main` moves (for example a hotfix tagged while the release PR is open): a
   stale green is possible, so rerun the job or push to the release branch to refresh it. The
   build jobs still run `scripts/set-version.sh <version>`; it changes nothing when the files are
   already stamped, and builds keep `--locked` working. The iOS bundle versions are not stamped
   in git: `app/fastlane/Fastfile` takes the short version from `tauri.conf.json`, overrides the
   build number and fails the lane when the built IPA differs.
4. Tags pushed with `GITHUB_TOKEN` do not trigger workflows, so the artifacts are built in the
   same run (`needs: tag`), not by an `on: push: tags` workflow.

`publish` creates the GitHub Release `Overseer vX.Y.Z` once the server, Linux and Windows jobs
succeeded; the macOS bundle and the Android APK are attached when their jobs succeeded. Nothing
goes to the App Store or the Play Store from a release run.

### Store releases

Store reviews are slow and every submission needs Apple's and Google's approval, so stores get a
release only when the maintainer asks for one: Actions > Store > Run workflow, branch `main`.
Inputs:

- `tag`: the release to ship, default the latest `vX.Y.Z` tag on `main`. The run fails unless it
  was started from `main` and the tag is an ancestor of `main`.
- `release_notes`: the App Store "What's New" text of the version, required for every update
  (default "Bug fixes and improvements.").
- `ios`, `android`: which stores to ship to (both by default).
- `track`: the Play track, `internal`, `alpha`, `beta` or `production` (default `alpha`).

Both jobs check out the tag and stamp its version with `scripts/set-version.sh`. `ios` builds the
IPA and submits it for App Store review with `automatic_release: false`: after approval, release it
by hand in App Store Connect. The listing (description, screenshots, privacy) lives in App Store
Connect and is not uploaded; only the version's "What's New" text is sent. The same build is available to the internal TestFlight testers.
`android` builds the Play App Bundle with `OVERSEER_DISTRIBUTION=play` (no in-app updater), checks
it with `scripts/android-aab.sh` and `scripts/android-16k.sh`, and uploads it with
`r0adkll/upload-google-play` to the chosen track (`alpha` is closed testing), status `completed`. One-time setup for both is
in [docs/mobile.md](mobile.md#one-time-setup).

### When a release job fails

Use "Re-run failed jobs" on the run: the `tag` job's outputs are reused and `publish` runs again
(`allowUpdates`, same assets replaced). "Re-run all jobs" would find the commit already tagged
and stop at `gate`, building nothing.

## Secrets

Settings > Secrets and variables > Actions. None is required: each one turns a feature on, and a
job that needs a missing one is skipped or builds unsigned, with a warning annotation from the
`tag` job (or from the macOS signing step). The store jobs are the exception: they run only when
asked, so a missing secret fails them.

| Secret | Enables | Without it |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | minisign signatures of the self-updating bundles (`.AppImage.sig`, `-setup.exe.sig`, `.app.tar.gz.sig`) and `latest.json`, so installed apps update themselves | bundles built without updater artifacts, no `.sig`, no `latest.json`: installed apps do not see the release |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password of that key (leave unset if the key has none) | |
| `MACOS_SELF_SIGN_P12_BASE64`, `MACOS_SELF_SIGN_P12_PASSWORD` | stable self-signed macOS code signature ([macOS signing](#macos-signing)) | ad-hoc signature: macOS forgets the Local Network grant on every update |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | Developer ID signature of the macOS app (wins over the self-signed one) | self-signed or ad-hoc, as above |
| `WINDOWS_SELF_SIGN_PFX_BASE64`, `WINDOWS_SELF_SIGN_PFX_PASSWORD` | self-signed `.msix` packages for x64 and arm64 and `overseer-msix.cer` in the release ([MSIX](#msix)) | no `.msix`, the NSIS installers ship as before |
| `APPLE_KEY_ID`, `APPLE_ISSUER_ID`, `APPLE_KEY_CONTENT` | App Store Connect API key (Developer role; `APPLE_KEY_CONTENT` is the `.p8` text): notarization of the Developer ID app; `APPLE_ISSUER_ID` is shared with the store key | with a Developer ID set, `app-macos` fails instead of shipping an app Gatekeeper blocks |
| `HOMEBREW_TAP_DEPLOY_KEY` | the `tap` job: the private half of an SSH deploy key with write access on `basilelt/homebrew-tap` only (create with `ssh-keygen -t ed25519`, add the public half under the tap's Settings > Deploy keys with "Allow write access") | `tap` skipped, the tap keeps the previous version |
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | `app-android`: signed release APK ([docs/mobile.md](mobile.md#android-once-by-whoever-holds-the-release-key)) | `app-android` skipped, no APK in the release |
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | also the store `android` job: the AAB is signed with the same key, the Play upload key | store `android` fails |
| `PLAY_SERVICE_ACCOUNT_JSON` | store `android`: upload of the App Bundle to Google Play ([docs/mobile.md](mobile.md#google-play-once-by-the-play-console-account-holder)) | store `android` fails |
| `APPLE_TEAM_ID`, `ASC_KEY_ID`, `APPLE_ISSUER_ID`, `ASC_KEY_CONTENT` (App Manager or Admin key, needed to submit for review), `IOS_CERTIFICATE`, `IOS_CERTIFICATE_PASSWORD`, `IOS_MOBILE_PROVISION` | store `ios`: App Store submission ([docs/mobile.md](mobile.md#ios-once-by-the-apple-developer-account-holder)) | store `ios` fails |

Repository variables (same page, Variables tab; `PLAY_TRACK` and `PLAY_RELEASE_STATUS`, used by
the former automatic Play upload, are no longer read and can be deleted):

| Variable | Use |
|---|---|
| `ANDROID_CERT_SHA256` | SHA-256 fingerprint of the Android release certificate; `scripts/android-apk.sh` fails the release APK when another key signed it (the PR `android` job pins the runner debug keystore the same way). Unset: the signature is still verified, the fingerprint is not pinned |

`GITHUB_TOKEN` covers everything else (tags, releases, the release gate's check-run lookups).
Release downloads need no token: the repository is public.

Windows code signing is not configured: no secret exists for it and the NSIS installer is
unsigned (SmartScreen warning). It would go through a `bundle.windows.signCommand`
(`osslsigncode` on Linux) fed by two new secrets.

### MSIX

The `.msix` packages wrap the same `overseer-app.exe` as the NSIS installer with
`packaging/msix/AppxManifest.xml` (full trust desktop app, identity `io.github.basilelt.overseer`,
publisher `CN=basilelt`), packed with `makeappx` and signed with `signtool` from the Windows SDK of
the runner (`scripts/msix.sh`). Windows installs an MSIX only when its signer is trusted, so each
release also carries the public `overseer-msix.cer`; users import it once into Local Machine >
Trusted People. An MSIX install never self-updates: the app sees its `WindowsApps` path and shows
the release link like a `.deb`/`.rpm` install. Windows keeps an MSIX app's `%APPDATA%` apart, so
switching between the MSIX and the NSIS install starts with fresh settings.

Create the certificate once, on any machine with `openssl`:

```sh
MSIX_PFX_PASSWORD='<password>' scripts/msix-cert.sh ~/overseer-msix
```

Put `overseer-msix.pfx.b64` in `WINDOWS_SELF_SIGN_PFX_BASE64` and the password in
`WINDOWS_SELF_SIGN_PFX_PASSWORD`, and keep the `.pfx` private. A new certificate means users
import the new `.cer` again. The publisher in the manifest must equal the certificate subject;
set `MSIX_PUBLISHER` for both scripts to change it.

## App updates

The app updates itself with `tauri-plugin-updater` from the GitHub releases (`app/README.md`,
"Updates"). The release side:

- The three desktop app jobs get `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. When the key is set they build with
  `--config app/src-tauri/tauri.updater.conf.json` (`bundle.createUpdaterArtifacts: true`), which
  signs the self-updating bundles with minisign. `tauri.conf.json` never turns updater artifacts
  on by itself, so without the key the build succeeds and only the updater files are missing.
- Updater files attached to the release: `Overseer_<v>_amd64.AppImage.sig`,
  `Overseer_<v>_aarch64.AppImage.sig`, `Overseer_<v>_x64-setup.exe.sig`,
  `Overseer_<v>_arm64-setup.exe.sig`, and `Overseer_<v>_aarch64.app.tar.gz` and
  `Overseer_<v>_x64.app.tar.gz` with their `.sig` (renamed from `bundle/macos/Overseer.app.tar.gz`). `.deb`, `.rpm` and `.dmg` are not
  self-updating.
- `publish` then runs `scripts/latest-json.sh <version>`: it reads `gh release view --json
  assets,body,publishedAt`, downloads the `.sig` files and writes `latest.json` with `version`,
  `notes` (the release body), `pub_date` and `platforms` (`darwin-aarch64`, `darwin-x86_64`,
  `windows-x86_64`, `windows-aarch64`, `linux-x86_64`, `linux-aarch64`). URLs are API
  asset URLs (`https://api.github.com/repos/.../releases/assets/<id>`), which the app downloads
  with `Accept: application/octet-stream`; on a public repository they need no token. A platform
  without a signed file is left out with a warning; no platform at all fails the step. It is
  uploaded with `gh release upload --clobber`, so a re-run replaces it. The step is skipped when
  `TAURI_SIGNING_PRIVATE_KEY` is empty.
- The app never has a fixed manifest URL (asset ids change on every upload): it asks
  `GET /repos/basilelt/overseer/releases/latest` for the `latest.json` asset first.

The public key is `plugins.updater.pubkey` in `app/src-tauri/tauri.conf.json`; it must match
`TAURI_SIGNING_PRIVATE_KEY`. Create a pair with `pnpm --dir app tauri signer generate -w
overseer.key`, put the private key and its password in the two secrets, and the public key in
`tauri.conf.json`. Rotating the key later means a release carrying the new public key, signed
with the old key, before signing with the new one.

## Homebrew tap

`basilelt/homebrew-tap` (tap `basilelt/tap`) has the `overseer` cask (the `.dmg`) and formula (the
daemon binaries for macOS and Linux, both arches). The `tap` job, after `publish` and only when
`HOMEBREW_TAP_DEPLOY_KEY` is set:

1. checks out the tap over SSH with the deploy key;
2. runs `scripts/update-tap.sh <version> tap` (bash, `gh`, `jq`): public download URLs
   (`https://github.com/basilelt/overseer/releases/download/v<version>/<asset>`) from
   `gh release view --json assets`, sha256 from the release `SHA256SUMS`. The cask is left
   unchanged when the release has no `.dmg` (the macOS job may fail);
3. commits `overseer <version>` as `github-actions[bot]` and force-pushes it to
   `bump/overseer-<version>`, or does nothing when the files did not change, so re-runs are safe;
4. the tap's `.github/workflows/tests.yml` then runs on that branch: `brew style`,
   `brew audit --strict --online` of the formula (and of the cask on macOS), `brew install` and
   `brew test` on macOS arm64, macOS Intel and Linux, and on macOS `brew install --cask` followed
   by `spctl --assess`, which fails unless Gatekeeper accepts the installed app. Only when every job is green does it fast-forward
   `main` to the branch and delete it, so users never get a formula or cask that fails brew's checks.

The tap repository must exist first (public, with one commit on its default branch; the job
creates `Casks/` and `Formula/`). Users: `brew install --formula basilelt/tap/overseer` and/or
`brew install --cask basilelt/tap/overseer`; the fully qualified name taps the repository and
makes Homebrew trust the formula or cask, so users never run `brew trust` themselves.

Every release asset also gets a GitHub build provenance attestation (the `publish` job attests
each file listed in `SHA256SUMS`): `gh attestation verify <file> --repo basilelt/overseer`
checks that a download was built by this workflow from this repository.

## One-time setup

- Settings > Actions > General: Actions allowed, and "Workflow permissions" not capped below what
  the jobs request (`contents: write` on `tag`, `publish` and `mobile-init.yml`; every other job
  has `contents: read`).
- Branches: `next` created from `main` (`git push origin main:next`) and set as the default
  branch (Settings > General > Default branch, or
  `gh api -X PATCH repos/basilelt/overseer -f default_branch=next`), so pull requests and
  Dependabot target it.
- Rulesets: create them from `.github/rulesets/main.json`, `.github/rulesets/next.json`,
  `.github/rulesets/no-delete.json` and `.github/rulesets/release-tags.json` with
  `gh api -X POST repos/basilelt/overseer/rulesets --input <file>` (later changes:
  `gh api -X PUT repos/basilelt/overseer/rulesets/<id> --input <file>`, ids from
  `gh api repos/basilelt/overseer/rulesets --jq '.[] | [.id, .name] | @tsv'`). `main` and `next`
  each require a PR, merge commits only, and the `CI` checks (`changes`, `actionlint`, `rust`,
  `app`, `perf`, `windows`, `android`, `store-guard`, `macos`, `gitleaks`, plus `release-version` on `main`; `integration_id` 15368 is the
  GitHub Actions app); jobs skipped by path filtering count as passed. A PR does not need to be up
  to date with its base: when it is behind, the merged tree differs from the tested one, and for
  `main` the release gate then runs CI again before tagging. Repository admins can bypass the `next`
  ruleset when merging a PR (`bypass_mode: pull_request`: no direct push), for exceptional cases;
  on `main` they bypass it always (`bypass_mode: always`, direct push included), for urgent CI or
  release fixes only. Both name their branch literally (`refs/heads/main`, `refs/heads/next`), not
  `~DEFAULT_BRANCH`, since the default branch is `next`. `no-delete` forbids deleting `main` and
  `next` with no bypass: the repository deletes head branches on merge, and an admin merging a
  `main` to `next` PR would otherwise delete `main` through the `main` bypass. `release-tags` forbids deleting `v*` and
  `latest` (repository admins can bypass); moving them is an update, so the `tag` job still can.
- The secrets and the variables above, as each feature is wanted.

### macOS signing

macOS keys Local Network (and every other privacy) grant to the app's designated requirement
(DR). An ad-hoc signature has no certificate, so its DR is the cdhash of that exact build: every
update is a new app to TCC, the toggle under Privacy & Security > Local Network belongs to an older
build and macOS does not prompt again for the same bundle id. The `app-macos` job therefore signs
with a stable identity when it has one, picked in this order:

1. Developer ID, when `APPLE_CERTIFICATE` (base64 .p12 with the Developer ID G2 intermediate),
   `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY` are all set: `scripts/macos-sign.sh
   import` loads the p12 into a temporary keychain (with `SIGN_TRUST=0`: no local trust override,
   the DR must stay `anchor apple generic`), `scripts/macos-sign.sh sign-resources` signs every
   Mach-O under `app/src-tauri/resources/server` (the bundled daemons) with the hardened runtime
   and a secure timestamp, and the Tauri CLI signs the app with the hardened runtime, then
   notarizes and staples it with the App Store Connect API key (`APPLE_API_KEY`,
   `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` from `APPLE_KEY_ID`, `APPLE_ISSUER_ID`,
   `APPLE_KEY_CONTENT`; the `.p8` is written to `$RUNNER_TEMP` and removed at the end).
2. Self-signed, when `MACOS_SELF_SIGN_P12_BASE64` and `MACOS_SELF_SIGN_P12_PASSWORD` are set:
   `scripts/macos-sign.sh import` decodes the p12 into a temporary keychain (partition list set
   for `codesign`, added to the search list, certificate marked trusted for code signing in the
   runner's System keychain) and the build runs with `APPLE_SIGNING_IDENTITY=<sha1>` and
   `tauri.selfsign.conf.json` (hardened runtime off: only notarization needs it, and a network
   client needs no entitlement). `NSLocalNetworkUsageDescription` stays in `Info.plist`.
3. Otherwise ad-hoc, with a warning annotation.

`scripts/macos-sign.sh verify` then fails the job unless `codesign -dv --verbose=4` shows
`Authority=Overseer Self-Signed` (or Developer ID, plus `xcrun stapler validate` and
`spctl --assess` reporting `source=Notarized Developer ID`) and `codesign -d -r-` gives a DR such as
`identifier "io.github.basilelt.overseer" and certificate root = H"d98c76cf…"`, with no `cdhash`.
The keychain is deleted at the end of the job. The `macos` CI job runs the same import and verify
with a throwaway certificate on two different builds and checks that both get the same DR.

The self-signed identity: CN `Overseer Self-Signed`, RSA 3072, 10 years, `codeSigning` extended
key usage, generated once with `openssl` and kept in a password manager; the p12 uses the legacy
(3DES) encryption that `security import` reads. Rotating it changes the DR, so users toggle
Overseer once more under Local Network after the first build signed with the new one. To create
one:

```sh
openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes -keyout key.pem -out cert.pem \
  -subj "/CN=Overseer Self-Signed/O=Overseer" \
  -addext "basicConstraints=critical,CA:FALSE" -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"
openssl pkcs12 -export -legacy -inkey key.pem -in cert.pem -name "Overseer Self-Signed" \
  -out overseer-self-signed.p12 -passout file:p12-password.txt
base64 -w0 overseer-self-signed.p12   # MACOS_SELF_SIGN_P12_BASE64
```

On each Mac, once, after installing the first stable-signed build: open Overseer, answer the
Local Network prompt (or switch Overseer on under Privacy & Security > Local Network; if an old
ad-hoc entry is shown, switch it off and on), then quit and reopen. Later updates signed with the
same certificate keep the grant. Caveat: Apple's TN3179 says Local Network privacy also tracks
the main executable's Mach-O UUID (new on every build) and only promises reliable tracking for
Apple-issued identities; a stable self-signed DR fixes the ad-hoc cdhash case, Developer ID
remains the complete fix, and the Terminal workaround in app/README.md still applies if a
self-signed update ever loses the grant. Local Network is not in the TCC database, so
`tccutil reset` does not clear it; use the toggle. A self-signed app is not notarized, so
Gatekeeper treats it as unidentified: after installing it from a downloaded `.dmg` or the cask,
run `xattr -dr com.apple.quarantine /Applications/Overseer.app`. The Developer ID build needs none
of this.

## Limitations

- **macOS app**: Tauri cannot bundle a `.app`/`.dmg` on Linux (it needs the Apple SDK, `codesign`,
  `hdiutil`), so `app-macos` runs on `macos-26`, Developer ID signed and notarized, or
  self-signed or ad-hoc without the Apple secrets (see [macOS signing](#macos-signing)). A failure
  there blocks the release, like every other desktop app job.
- **macOS runner label**: every macOS job uses `macos-26`. If GitHub retires or renames that
  label, those jobs wait for a runner that never comes; change it in `ci.yml`, `release.yml`,
  `store.yml` and `mobile-init.yml` together.
- **macOS daemon binaries** are built natively on `macos-26` and ad-hoc signed by the linker, not
  Developer ID signed: `curl`-installed binaries are not quarantined, so remote install over SSH
  is unaffected. Two binaries are shipped, no universal one.
- **Windows app**: NSIS only. MSI needs WiX, which runs on Windows only. WebView2 is installed by
  the default bootstrapper when missing (built into Windows 11). The NSIS installer is unsigned
  (SmartScreen warning).
- **Windows daemon**: `overseer.exe` is attached to the release; the app does not bundle it, so
  local mode on Windows needs `overseer.exe` on the `PATH` or in the app folder.
- No Windows tests run, only the native clippy check; macOS runs the `procs`, `ports` and
  `review::watch` tests. Linux runs the full suite.
- Fallback if `cargo-xwin` breaks: build `overseer.exe` natively on `windows-latest`, or
  `x86_64-pc-windows-gnu` with `cargo zigbuild`; the Tauri app would then need a native
  `windows-latest` job like `app-windows-arm`.

## Local equivalents

```sh
make lint && timeout 1500 make test && make app-test
scripts/ci-changes.sh origin/next HEAD   # which CI jobs a PR from this branch would run
scripts/ci-changes.test.sh && scripts/ci-apt.test.sh   # path filter and cached apt install (Docker)
make dist-cross          # needs zig, cargo-zigbuild, cargo-xwin, clang, lld, llvm, musl-tools and the rustup targets
make dist-darwin         # on a Mac: both macOS daemons (aarch64 and x86_64 rustup targets)
make app-resources       # copies dist/ binaries into app/src-tauri/resources/server/
scripts/set-version.sh 1.2.3   # stamps every manifest; do not commit the result
actionlint
```
