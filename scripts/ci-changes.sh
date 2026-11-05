#!/usr/bin/env bash
set -uo pipefail

jobs="rust app windows android macos_signing macos_procs actionlint store_guard tap ci_scripts"
for j in $jobs; do declare "$j=false"; done
all=false
wide=false

if [ "${SKIP:-}" = true ]; then
  echo "::notice title=ci changes::skipped by the release gate"
elif [ "${EVENT:-pull_request}" != pull_request ] && [ "$EVENT:${REF:-}" != push:refs/heads/next ]; then
  all=true
elif ! files=$(git diff --name-only "${1:-HEAD^1}" "${2:-HEAD}"); then
  echo "::warning title=ci changes::git diff failed, running every job"
  all=true
else
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    echo "$f"
    case $f in
      docs/* | *.md | .github/ISSUE_TEMPLATE/*) continue ;;
    esac
    case $f in
      .github/workflows/ci.yml | .github/actions/* | scripts/ci-*.sh | Makefile) wide=true ;;
    esac
    case $f in
      src/* | api/* | tests/* | board/* | Cargo.toml | Cargo.lock | build.rs) rust=true app=true windows=true ;;
    esac
    case $f in
      app/src-tauri/gen/* | app/fastlane/*) ;;
      app/* | scripts/perf-cpu.sh) app=true ;;
      contract/*) app=true rust=true ;;
    esac
    case $f in
      app/src-tauri/* | app/package.json | app/pnpm-lock.yaml | scripts/android-*.sh) android=true ;;
    esac
    case $f in
      src/procs.rs | src/ports.rs | src/review/watch.rs | Cargo.toml) macos_procs=true ;;
    esac
    case $f in
      .github/workflows/release.yml | scripts/macos-sign.sh | app/src-tauri/tauri*.conf.json | app/src-tauri/Info.plist) macos_signing=true ;;
    esac
    case $f in
      .github/*) actionlint=true ;;
    esac
    case $f in
      .github/actions/* | scripts/ci-*.sh) ci_scripts=true ;;
    esac
    case $f in
      scripts/update-tap.sh | scripts/update-tap.test.sh) tap=true ;;
    esac
    case $f in
      .github/* | app/fastlane/* | app/src-tauri/gen/apple/Assets.xcassets/* | scripts/* | Makefile) store_guard=true ;;
    esac
  done <<<"$files"
fi

for j in $jobs; do
  [ "$all" = true ] && declare "$j=true"
  [ "$wide" = true ] && [ "${j#macos_}" = "$j" ] && declare "$j=true"
  echo "$j=${!j}" | tee -a "${GITHUB_OUTPUT:-/dev/null}"
done
