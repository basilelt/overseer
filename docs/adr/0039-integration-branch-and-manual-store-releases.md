# 0039 Integration branch and manual store releases

Status: accepted (supersedes the store decisions of 0032)

## Context
Every merge to `main` was a release (ADR 0027): a tag, every desktop bundle, the daemon binaries,
the Android APK, the Homebrew tap and a TestFlight upload. With one PR per feature or fix, that
meant a full release run, macOS and Windows runners included, for each small change, and a
stream of tiny versions. The App Store and Play Store add a review for every build, so shipping
there on every merge is neither possible nor wanted. ADR 0032 kept iOS on TestFlight only and
forbade any store submission.

## Decision
- **`next` is the integration branch** and the default branch on GitHub. Feature and fix branches
  start from it and merge into it through draft PRs with merge commits, CI as before. Merging into
  `next` releases nothing. Hotfixes take the same path.
- **A release is a PR from `next` to `main`**, opened by the maintainer when enough is batched,
  QA'd on the merged tree with the full path of `docs/qa-path.md`, merged with a merge commit.
  The release gate, the automatic `vX.Y.Z` tag from the Conventional Commits of the whole batch,
  the GitHub Release, the desktop bundles, the Obtainium APK and the Homebrew tap are unchanged.
  `main` then has a merge commit `next` lacks; a PR from `main` to `next` syncs it right after
  each release (strict status checks require it, and the ruleset forbids pushing to `next`).
- **Rulesets**: `main` and `next` each require a PR, merge commits and the CI checks
  (`.github/rulesets/main.json`, `next.json`). Both name their branch literally, since
  `~DEFAULT_BRANCH` now means `next`.
- **Stores are manual**: `.github/workflows/store.yml`, `workflow_dispatch` only, refuses to run
  from any ref but `main` and ships a tag reachable from `main` (default the latest). iOS: the
  fastlane `app_store` lane builds the IPA as before and submits it with `upload_to_app_store`
  (`submit_for_review`, `automatic_release: false`, metadata and screenshots kept in App Store
  Connect); the build also reaches the internal TestFlight group. Android: an AAB signed with the
  release key (the Play upload key), uploaded with `upload_to_play_store` to a chosen track
  (default `production`) using a Play service account. `release.yml` no longer builds iOS.
- **The store guard changes meaning**: `scripts/store-guard.sh` now fails when a store submission
  appears outside `store.yml` and `app/fastlane/`, or when `store.yml` gets a trigger other than
  `workflow_dispatch`, so no automatic run can reach a store.

## Rejected
- **Release on a schedule from `main`**: still one CI run per merge on `main`, and the release
  contents would depend on the clock rather than on a decision.
- **Store upload in `release.yml` behind an input**: `release.yml` runs on push, so the only safe
  default is off, and a separate workflow keeps the macOS runner and store secrets out of the
  release run entirely.
- **Keeping TestFlight in every release**: a macOS run per release for builds only testers see;
  the store run already feeds TestFlight.

## Consequences
- Fewer, bigger releases: CI runs on each PR to `next` and once on the release PR; the release
  run builds only when `main` moves.
- `since` of the tour steps is the next minor at the time the release PR merges, shared by every
  `feat` of the batch.
- Store builds lag the GitHub releases. iOS `CFBundleVersion` restarts with the `store.yml` run
  number, so only tags created after this change are shipped to the App Store.
- One-time human steps: the App Store listing, Play Console app, first AAB uploaded by hand, Play
  App Signing (upload the existing release key so Obtainium and Play installs can update each
  other), the service account (docs/mobile.md).
