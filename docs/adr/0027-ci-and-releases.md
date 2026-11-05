# 0027 CI and releases

Status: accepted

Numbered 0027 because 0026 is taken on another branch.

## Context
The first CI (`ci.yml`) ran on GitHub-hosted Linux, arm64 Linux, macOS and Windows runners, with
old action versions, and nothing was released: binaries were workflow artifacts, the version was
bumped by hand. The app needs daemon binaries for Linux and macOS hosts inside its bundle (remote
mode, ADR 0025), and installers for Linux, Windows and macOS.

The project then had self-hosted Linux runners (LXC containers, 8 cores, 8 GB RAM, 50 GB disk,
Debian 13, ephemeral registration, shared cache disk, no sudo for jobs); no macOS runner. Their
conventions: heavy jobs on the self-hosted runners, light ones on `ubuntu-slim`, tools
preinstalled in the runner template, major action tags, a release on every merge with
`mathieudutour/github-tag-action`.

## Decision
- **Runners**: every build and test job runs on the self-hosted Linux runners; tag and publish on
  `ubuntu-slim`; only the macOS app bundle runs on GitHub-hosted `macos-26`. (Superseded: see
  the addendum.)
- **Tools in the template, not in the jobs**: jobs cannot `sudo apt-get`. Install steps run only on
  GitHub-hosted runners; self-hosted jobs check their tools first (`scripts/ci-runner-check.sh`)
  and fail fast.
- **Cross-compilation from Linux**:
  - Linux musl x86_64/aarch64 and macOS x86_64/aarch64 daemons with `cargo-zigbuild` (Zig as C
    compiler and linker for `rusqlite`'s bundled SQLite). No musl cross-gcc and no arm64 runner
    needed. macOS links against Zig's libSystem stubs, without an Apple SDK: enough for the daemon,
    which links no Apple framework. Two macOS binaries, as the app expects
    (`server/darwin-<arch>/overseer`).
  - Windows with `cargo-xwin`, target `x86_64-pc-windows-msvc` (clang-cl, lld-link and the MSVC
    CRT/Windows SDK downloaded by xwin). MSVC rather than `x86_64-pc-windows-gnu` because ADR 0023
    and users target MSVC (ConPTY through `portable-pty`, no MinGW runtime DLLs), and because
    Tauri's only supported Linux-to-Windows build is `tauri build --runner cargo-xwin --target
    x86_64-pc-windows-msvc`. The app's dependency tree has `aws-lc-sys` (through `russh`), which
    needs `cmake` and `nasm` on this path; the daemon has no such dependency. The GNU target with
    `cargo zigbuild` stays the documented fallback for `overseer.exe` alone.
  - Windows app: NSIS installer only; MSI needs WiX, which only runs on Windows. WebView2 with the
    default download bootstrapper.
- **macOS app on GitHub-hosted macOS**: Tauri needs the Apple SDK, `codesign` and `hdiutil` to
  produce a `.app`/`.dmg`. The job is `continue-on-error` and unsigned until signing secrets exist.
- **One run per release**: `release.yml` on push to `main` calls `ci.yml`, then tags, then builds
  and publishes. Tags pushed with `GITHUB_TOKEN` trigger no workflow, so a separate
  `on: push: tags` workflow would never run.
- **Versioning**: Conventional Commits decide the bump (`default_bump: patch`). The tag is the
  source of truth; `scripts/set-version.sh` stamps it into every manifest and lock entry in the CI
  workspace, nothing is committed back (no bot commits on `main`, no loop). With no `vX.Y.Z` tag
  yet, the first tag is the root `Cargo.toml` version (`custom_tag`), so releases start at
  `v0.2.0`. `vX`, `vX.Y` and `latest` move with each release.
- **Release assembly**: build jobs upload workflow artifacts; one `publish` job creates the
  release with the changelog of the tag action, all files and `SHA256SUMS`. It requires the
  server, Linux and Windows jobs; the macOS bundle is optional.
- **Disk**: no target-directory cache (several GB per build on disks with 10 to 31 GB free); only
  the cargo registry is cached; jobs delete `target/` at the end.

## Consequences
- The runner template must be updated before the first run, and the Rust toolchain on the
  runners only moves with the template.
- No tests run on Windows or macOS; Windows gets a clippy check, macOS nothing before release.
- macOS binaries are ad-hoc signed only and the `.dmg` is unsigned; Windows installers are
  unsigned. Signing secret names are listed in `docs/ci.md`.
- The versions committed in the manifests no longer say which release is current; `overseer
  version` of a release build does.
- Replaces the old `ubuntu-24.04-arm`/macOS/Windows jobs of ADR 0024's "Decision" on where macOS
  and aarch64 binaries are built.

## Addendum: GitHub-hosted runners

The repository moved to a public repository on a personal account, with no self-hosted runner.
Every job now runs on a GitHub-hosted runner; the rest of this decision (one run per release,
release gate, versioning from the tag, cross-compilation from Linux, release assembly) is
unchanged.

- **Runners**: Linux build and test jobs on `ubuntu-24.04` (4 vCPUs, 16 GB RAM, about 14 GB free
  disk); light jobs stay on `ubuntu-slim`; macOS jobs stay on `macos-26`. The CI Windows check
  runs natively on `windows-latest` (`cargo clippy`, no cross toolchain). The Windows installer
  and `overseer.exe` stay cross-built on Linux with `cargo-xwin`: `make dist-cross` needs it for
  `overseer.exe` anyway, and `make app-resources` is not available on `windows-latest` without
  installing `make`.
- **Tools in the jobs**: there is no template any more. Each job installs what it needs
  (`dtolnay/rust-toolchain`, `pnpm/action-setup` and `actions/setup-node`, `sudo apt-get` for the
  Tauri libraries, `tmux`, Xvfb and the cross tools, `mlugg/setup-zig`, `taiki-e/install-action`
  for `cargo-zigbuild` and `cargo-xwin`, `actions/setup-java`); the Android SDK and NDK are the
  image's. `scripts/ci-runner-check.sh` still checks the result before the build.
- **Caches**: `actions/cache` for the cargo registry, Gradle and the xwin SDK; still no target
  directory cache (the Actions cache is 10 GB per repository). Jobs no longer delete their
  workspace: hosted runners are discarded after each job.
- **Secrets are optional**: a job that needs a secret is skipped, or builds unsigned, while the
  secret is unset, so the pipeline goes green on a repository with none (`docs/ci.md`,
  "Secrets"). The Homebrew tap is pushed with an SSH deploy key scoped to the tap
  (`HOMEBREW_TAP_DEPLOY_KEY`) instead of an organization GitHub App, and its formulas use public download URLs.
- **First push**: the gate also skips a release when `HEAD` already carries a `vX.Y.Z` tag, so the
  repository's first commit, pushed with its tag `v0.26.3`, builds nothing.

Consequences: release runs take longer (4 vCPUs instead of 8, no warm runner cache), and the CPU
budgets of the `perf` job, calibrated on the 8-core runners, may be tight on hosted runners; they
are kept as they are, a failure is investigated as a slowdown first.
