# CI and releases

Three workflows in `.github/workflows/` (ADR 0027; mobile builds ADR 0032 and
[docs/mobile.md](mobile.md)). Every job runs on a GitHub-hosted runner; the repository needs no
self-hosted runner.

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | non-draft pull requests (`opened`, `synchronize`, `reopened`, `ready_for_review`), manual (`workflow_dispatch`), and called by `release.yml` | lint, tests, Windows check, debug APK |
| `release.yml` | push to `main` | CI, version tag, binaries, app bundles, GitHub Release, TestFlight, Homebrew tap |
| `mobile-init.yml` | manual | generates `app/src-tauri/gen/{android,apple}` and commits them to the branch |

Concurrency: `ci-pr-<PR number or ref>` cancels the previous run of the same PR;
`release-refs/heads/main` never cancels a running release (a newer push waits).

Runner budget: CI runs nothing on draft PRs, only the jobs the change needs on ready PRs
([Path filtering](#path-filtering)), and is not repeated on `main` when the merged PR already passed
it ([Release gate](#release-gate)). See [Workflow](#workflow) for how branches reach `main`.

A release run with no secret configured goes green: it publishes the daemon binaries and the
desktop bundles, unsigned or ad-hoc signed, and skips with a warning everything that needs a
secret ([Secrets](#secrets)).

## Workflow

One feature or fix = one branch = one PR:

1. Branch `<type>/<description>` from an up-to-date `main`. Push once the local checks of
   `AGENTS.md` pass, batching fixes into one push.
2. Open a draft PR to `main` with a Conventional Commits title: no CI runs while it is a draft.
3. Mark it ready when the branch is complete, so CI runs once. Further pushes rerun it (the
   previous run is cancelled).
4. Merge with a merge commit (not squash, not rebase), after merging `main` into the branch if
   `main` moved and CI is green again. The release then skips CI (the gate sees the same tree
   already passed).
5. Each merge to `main` makes one release: `tag` bumps the current `vX.Y.Z` tag from the
   Conventional Commits of the merge ([Versioning](#versioning)), then the builds and the GitHub
   Release follow. A docs-only merge makes no release.

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
| `store-guard` | what `scripts/store-guard.sh` scans: `.github/`, `app/fastlane/`, `scripts/`, `Makefile` |

- `docs/`, `*.md` and `.github/ISSUE_TEMPLATE/` never trigger a job: a docs-only PR runs `changes`
  alone.
- `.github/workflows/ci.yml`, `scripts/ci-changes.sh`, `scripts/ci-runner-check.sh` and `Makefile`
  trigger every job except `macos`, which only follows its own paths: macOS runners are the
  scarcest and slowest to start, so it runs on pull requests and manual runs only, as one job.
- A failed `git diff` runs every job.
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
| ci | `actionlint` | `ubuntu-slim` | `actionlint` (downloaded by its install script; `ubuntu-slim` has no Docker) |
| ci | `rust` | `ubuntu-24.04` | `cargo fmt --check` (root and `api/`), `make lint`, `timeout 1500 make test` |
| ci | `app` | `ubuntu-24.04` | `make app-test` (vitest with coverage, svelte-check, Tauri crate tests and clippy) |
| ci | `perf` | `ubuntu-24.04` | release build of the app against the mock backend, `scripts/perf-cpu.sh` CPU budget (30 s idle, 30 s streaming) under Xvfb |
| ci | `windows` | `windows-latest` | native `cargo clippy --target x86_64-pc-windows-msvc --lib --bins -- -D warnings` |
| ci | `android` | `ubuntu-24.04` | debug APK for arm64 (`pnpm tauri android build --debug --apk --target aarch64`), artifact `android-debug-apk` |
| ci | `macos` | `macos-26` | throwaway self-signed identity on two builds must give one designated requirement; `cargo test --lib -- procs:: ports:: review::watch::`; pull requests and manual runs only |
| ci | `store-guard` | `ubuntu-slim` | `scripts/store-guard.sh`: no App Store / Play Store submission anywhere |
| release | `gate` | `ubuntu-slim` | `scripts/release-gate.sh` and the tagged-`HEAD` check ([Release gate](#release-gate)); skipped for `github-actions[bot]` and `[skip ci]` |
| release | `ci` | calls `ci.yml` | the jobs above, or `changes` alone when the gate says `skip_ci` |
| release | `tag` | `ubuntu-slim` | next `vX.Y.Z` tag, moves `vX`, `vX.Y`, `latest`; checks which optional secrets are set |
| release | `servers` | `ubuntu-24.04` | `make dist-cross`: daemon for Linux musl x86_64/aarch64 and Windows x86_64 (MSVC), `overseer-api` Linux musl x86_64 |
| release | `servers-macos` | `macos-26` | `make dist-darwin`: daemon for macOS x86_64/aarch64, built natively so it links FSEvents (CoreServices needs the Apple SDK) |
| release | `app-linux` | `ubuntu-24.04` | `.deb`, `.AppImage`, `.rpm` |
| release | `app-windows` | `ubuntu-24.04` | NSIS installer `*_x64-setup.exe`, cross-built with `cargo-xwin` |
| release | `app-macos` | `macos-26` | Apple Silicon (`aarch64-apple-darwin`) `.dmg`, no Intel build, `continue-on-error` |
| release | `app-android` | `ubuntu-24.04` | signed universal APK (arm64-v8a, armeabi-v7a) `Overseer_<v>_android.apk` + `.sha256`; skipped without the keystore secrets |
| release | `app-ios` | `macos-26` | fastlane `internal_testflight` lane: match (readonly), `tauri ios build`, `upload_to_testflight` with `skip_submission`; `continue-on-error`, skipped without the Apple secrets |
| release | `publish` | `ubuntu-slim` | GitHub Release with generated release notes, every artifact and `SHA256SUMS`, then the updater manifest `latest.json` when the updater key is set |
| release | `tap` | `ubuntu-24.04` | `scripts/update-tap.sh` writes the cask and formula of `basilelt/homebrew-tap` and pushes `overseer <version>`; skipped without `HOMEBREW_TAP_DEPLOY_KEY` |

The app jobs get the server binaries from the `servers*` artifacts and copy them into
`app/src-tauri/resources/server/<os>-<arch>/overseer` (`make app-resources`), so every app
bundle can install the daemon on Linux and macOS hosts in remote mode.

The Windows installer stays cross-built on Linux: `servers` needs `cargo-xwin` on Linux anyway for
`overseer.exe` (`make dist-cross`), and `make app-resources` is a POSIX `make` target that
`windows-latest` does not ship the tools for. Only the clippy check runs on `windows-latest`,
where it needs nothing but the Rust toolchain.

## Runner setup

Hosted runners have none of the project's tools, so each job installs what it needs:

| Tool | How | Jobs |
|---|---|---|
| Rust stable, `clippy`, `rustfmt`, cross targets | `dtolnay/rust-toolchain@stable` (`targets:` for musl, MSVC and Apple; the Android targets through `scripts/android-env.sh`) | every Rust job |
| Node 24, pnpm 11 | `pnpm/action-setup@v6`, then `actions/setup-node@v7` with `cache: pnpm` on `app/pnpm-lock.yaml` | app, perf, android, app-* |
| Tauri Linux libraries | `sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libxdo-dev libssl-dev pkg-config patchelf` | app, perf, app-linux (plus `file`) |
| `tmux` | apt | rust, app (the daemon's terminal tests drive real `tmux` sessions) |
| Xvfb | apt `xvfb dbus-x11` | perf |
| musl and Windows cross tools | apt `clang lld llvm musl-tools`, `mlugg/setup-zig@v2`, `taiki-e/install-action@v2` with `cargo-zigbuild,cargo-xwin` | servers |
| NSIS and the Windows app's native dependencies | apt `nsis clang lld llvm cmake nasm`, `cargo-xwin` | app-windows |
| JDK 21 | `actions/setup-java@v6` (temurin) | android, app-android, mobile-init |
| Android SDK and NDK | preinstalled on `ubuntu-24.04` (`ANDROID_HOME`, platform 36, build-tools); `scripts/android-env.sh` runs with `NDK_HOME=$ANDROID_NDK_HOME` (the image's default NDK, not the newest one) and adds the Rust Android targets | android, app-android, mobile-init |
| Xcode 26, fastlane, xcodegen | `maxim-lobanov/setup-xcode@v1`, `brew install` when missing | app-ios, mobile-init |

`git`, `ssh`, `ssh-keygen`, `curl`, `jq`, `gh` and Docker come with the images. The daemon's
Docker remote tests run only with `OVERSEER_DOCKER_TESTS=1` (`make test-remote`), not in CI.

Each job still starts with `scripts/ci-runner-check.sh`, which fails in a second with the list of
what is missing (commands, `pkg-config` modules, rustup targets) instead of deep in a build, and
puts `~/.cargo/bin` on the `PATH` of the next steps.

### Disk and caches

Standard hosted Linux runners of a public repository have 4 vCPUs, 16 GB of RAM and about 14 GB
of guaranteed free disk. A Tauri release build takes several GB, so:

- the `app`, `perf` and `app-linux` jobs first delete preinstalled toolchains they do not use
  (`/usr/share/dotnet`, `/usr/local/lib/android`, `/opt/ghc`, CodeQL); the Android jobs delete
  the same minus the Android SDK;
- target directories are not cached: one Tauri target would fill most of the 10 GB
  per-repository Actions cache. Only the cargo registry and git sources
  (`~/.cargo/registry/{index,cache}`, `~/.cargo/git/db`) go through `actions/cache@v6`, keyed on
  the runner OS and the three `Cargo.lock` files; Gradle (`~/.gradle/caches`, `~/.gradle/wrapper`) and the xwin
  MSVC CRT and Windows SDK (`~/.cache/cargo-xwin`) are cached the same way; the macOS jobs use
  `Swatinem/rust-cache@v2`;
- `CARGO_INCREMENTAL=0` everywhere.

### Perf budget on hosted runners

`scripts/perf-cpu.sh` measures the CPU of the app's process tree as a percentage of one core
(idle budget 3 %, streaming budget 80 %). The budgets were calibrated on 8-core runners; hosted
runners have 4 vCPUs, and WebKit renders in software under Xvfb. The work per frame does not
depend on the core count, but a slower core spends a larger share of its time on the same work,
so the streaming budget may be tight there. The budgets are not raised to make the job pass: if
`perf` fails on hosted runners only, profile the slowdown first.

## Actions

Major tags, no SHA pins; Dependabot bumps them monthly (`.github/dependabot.yml`).

`actions/checkout@v7`, `actions/setup-node@v7`, `actions/setup-java@v6`, `actions/cache@v6`,
`maxim-lobanov/setup-xcode@v1`, `actions/upload-artifact@v7`, `actions/download-artifact@v8`,
`pnpm/action-setup@v6`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2` (macOS only),
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
3. The build jobs run `scripts/set-version.sh <version>`, which writes the version into
   `Cargo.toml`, `api/Cargo.toml`, `app/src-tauri/Cargo.toml`, their `Cargo.lock` entries,
   `app/package.json` and `app/src-tauri/tauri.conf.json` in the CI workspace only. Nothing is
   committed back: the versions in git stay at their last manual value, the tag is the source of
   truth for releases. Builds keep `--locked` working because the lock entries are stamped too.
4. Tags pushed with `GITHUB_TOKEN` do not trigger workflows, so the artifacts are built in the
   same run (`needs: tag`), not by an `on: push: tags` workflow.

`publish` creates the GitHub Release `Overseer vX.Y.Z` once the server, Linux and Windows jobs
succeeded; the macOS bundle and the Android APK are attached when their jobs succeeded. The
TestFlight upload is independent of `publish`.

### When a release job fails

Use "Re-run failed jobs" on the run: the `tag` job's outputs are reused and `publish` runs again
(`allowUpdates`, same assets replaced). "Re-run all jobs" would find the commit already tagged
and stop at `gate`, building nothing.

## Secrets

Settings > Secrets and variables > Actions. None is required: each one turns a feature on, and a
job that needs a missing one is skipped or builds unsigned, with a warning annotation from the
`tag` job (or from the macOS signing step).

| Secret | Enables | Without it |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | minisign signatures of the self-updating bundles (`.AppImage.sig`, `-setup.exe.sig`, `.app.tar.gz.sig`) and `latest.json`, so installed apps update themselves | bundles built without updater artifacts, no `.sig`, no `latest.json`: installed apps do not see the release |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password of that key (leave unset if the key has none) | |
| `MACOS_SELF_SIGN_P12_BASE64`, `MACOS_SELF_SIGN_P12_PASSWORD` | stable self-signed macOS code signature ([macOS signing](#macos-signing)) | ad-hoc signature: macOS forgets the Local Network grant on every update |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | Developer ID signature of the macOS app (wins over the self-signed one) | self-signed or ad-hoc, as above |
| `HOMEBREW_TAP_DEPLOY_KEY` | the `tap` job: the private half of an SSH deploy key with write access on `basilelt/homebrew-tap` only (create with `ssh-keygen -t ed25519`, add the public half under the tap's Settings > Deploy keys with "Allow write access") | `tap` skipped, the tap keeps the previous version |
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | `app-android`: signed release APK ([docs/mobile.md](mobile.md#android-once-by-whoever-holds-the-release-key)) | `app-android` skipped, no APK in the release |
| `APPLE_TEAM_ID`, `APPLE_KEY_ID`, `APPLE_ISSUER_ID`, `APPLE_KEY_CONTENT`, `MATCH_GIT_URL`, `MATCH_PASSWORD`, `MATCH_SSH_PRIVATE_KEY` | `app-ios`: TestFlight upload ([docs/mobile.md](mobile.md#ios-once-by-the-apple-developer-account-holder)) | `app-ios` skipped |

One repository variable (same page, Variables tab):

| Variable | Use |
|---|---|
| `ANDROID_CERT_SHA256` | SHA-256 fingerprint of the Android release certificate; `scripts/android-apk.sh` fails the release APK when another key signed it. Unset: the signature is still verified, the fingerprint is not pinned |

`GITHUB_TOKEN` covers everything else (tags, releases, the release gate's check-run lookups).
Release downloads need no token: the repository is public.

Windows code signing is not configured: no secret exists for it and the NSIS installer is
unsigned (SmartScreen warning). It would go through a `bundle.windows.signCommand`
(`osslsigncode` on Linux) fed by two new secrets.

## App updates

The app updates itself with `tauri-plugin-updater` from the GitHub releases (`app/README.md`,
"Updates"). The release side:

- The three desktop app jobs get `TAURI_SIGNING_PRIVATE_KEY` and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. When the key is set they build with
  `--config app/src-tauri/tauri.updater.conf.json` (`bundle.createUpdaterArtifacts: true`), which
  signs the self-updating bundles with minisign. `tauri.conf.json` never turns updater artifacts
  on by itself, so without the key the build succeeds and only the updater files are missing.
- Updater files attached to the release: `Overseer_<v>_amd64.AppImage.sig`,
  `Overseer_<v>_x64-setup.exe.sig`, and `Overseer_<v>_aarch64.app.tar.gz` with its `.sig`
  (renamed from `bundle/macos/Overseer.app.tar.gz`). `.deb`, `.rpm` and `.dmg` are not
  self-updating.
- `publish` then runs `scripts/latest-json.sh <version>`: it reads `gh release view --json
  assets,body,publishedAt`, downloads the `.sig` files and writes `latest.json` with `version`,
  `notes` (the release body), `pub_date` and `platforms` (`darwin-aarch64` on the `.app.tar.gz`,
  `windows-x86_64`, `linux-x86_64`; no `darwin-x86_64`, Intel Macs get no app update). URLs are API
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
3. commits `overseer <version>` as `github-actions[bot]` and pushes, or does nothing when the
   files did not change, so re-runs are safe.

The tap repository must exist first (public, with one commit on its default branch; the job
creates `Casks/` and `Formula/`). Users: `brew tap basilelt/tap`, then `brew install overseer`
and/or `brew install --cask overseer`.

## One-time setup

- Settings > Actions > General: Actions allowed, and "Workflow permissions" not capped below what
  the jobs request (`contents: write` on `tag`, `publish` and `mobile-init.yml`; every other job
  has `contents: read`).
- Rulesets: create them from `.github/rulesets/main.json` and `.github/rulesets/release-tags.json`
  with `gh api -X POST repos/basilelt/overseer/rulesets --input <file>` (later changes:
  `gh api -X PUT repos/basilelt/overseer/rulesets/<id> --input <file>`). `main` requires a PR,
  merge commits only, and the `CI` checks (`changes`, `actionlint`, `rust`, `app`, `perf`,
  `windows`, `android`, `store-guard`, `macos`; `integration_id` 15368 is the GitHub Actions
  app); jobs skipped by path filtering count as passed. `release-tags` forbids deleting `v*` and
  `latest` (repository admins can bypass); moving them is an update, so the `tag` job still can.
- The secrets and the variable above, as each feature is wanted.

### macOS signing

macOS keys Local Network (and every other privacy) grant to the app's designated requirement
(DR). An ad-hoc signature has no certificate, so its DR is the cdhash of that exact build: every
update is a new app to TCC, the toggle under Privacy & Security > Local Network belongs to an older
build and macOS does not prompt again for the same bundle id. The `app-macos` job therefore signs
with a stable identity when it has one, picked in this order:

1. Developer ID, when `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD` and
   `APPLE_SIGNING_IDENTITY` are all set: passed to the Tauri CLI as is (notarization would add
   `APPLE_ID`, `APPLE_PASSWORD` and the team id; not wired yet).
2. Self-signed, when `MACOS_SELF_SIGN_P12_BASE64` and `MACOS_SELF_SIGN_P12_PASSWORD` are set:
   `scripts/macos-sign.sh import` decodes the p12 into a temporary keychain (partition list set
   for `codesign`, added to the search list, certificate marked trusted for code signing in the
   runner's System keychain) and the build runs with `APPLE_SIGNING_IDENTITY=<sha1>` and
   `tauri.selfsign.conf.json` (hardened runtime off: only notarization needs it, and a network
   client needs no entitlement). `NSLocalNetworkUsageDescription` stays in `Info.plist`.
3. Otherwise ad-hoc, with a warning annotation.

`scripts/macos-sign.sh verify` then fails the job unless `codesign -dv --verbose=4` shows
`Authority=Overseer Self-Signed` (or Developer ID) and `codesign -d -r-` gives a DR such as
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
`tccutil reset` does not clear it; use the toggle. Gatekeeper still treats the app as
unidentified (no notarization): after installing from a downloaded `.dmg` or the cask, run
`xattr -dr com.apple.quarantine /Applications/Overseer.app`.

## Limitations

- **macOS app**: Tauri cannot bundle a `.app`/`.dmg` on Linux (it needs the Apple SDK, `codesign`,
  `hdiutil`), so `app-macos` runs on `macos-26`, self-signed or ad-hoc (see
  [macOS signing](#macos-signing)), not notarized. It is `continue-on-error`: a failure there does
  not block the release.
- **macOS runner label**: every macOS job uses `macos-26`. If GitHub retires or renames that
  label, those jobs wait for a runner that never comes; change it in `ci.yml`, `release.yml` and
  `mobile-init.yml` together.
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
  `windows-latest` job (`pnpm tauri build --bundles nsis`, with `make` installed for
  `make app-resources`).

## Local equivalents

```sh
make lint && timeout 1500 make test && make app-test
scripts/ci-changes.sh origin/main HEAD   # which CI jobs a PR from this branch would run
make dist-cross          # needs zig, cargo-zigbuild, cargo-xwin, clang, lld, llvm, musl-tools and the rustup targets
make dist-darwin         # on a Mac: both macOS daemons (aarch64 and x86_64 rustup targets)
make app-resources       # copies dist/ binaries into app/src-tauri/resources/server/
scripts/set-version.sh 1.2.3   # stamps every manifest; do not commit the result
actionlint
```
