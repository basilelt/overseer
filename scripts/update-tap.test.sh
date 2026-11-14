#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
tap_dir=$(brew --repository)/Library/Taps/overseer-ci/homebrew-tap
trap 'rm -rf "$work" "$tap_dir"; rmdir "$(dirname "$tap_dir")" 2>/dev/null' EXIT

mkdir -p "$work/bin"
cat >"$work/bin/gh" <<'SH'
#!/usr/bin/env bash
v=9.8.7
names="overseer-$v-aarch64-apple-darwin overseer-$v-x86_64-apple-darwin overseer-$v-aarch64-unknown-linux-musl overseer-$v-x86_64-unknown-linux-musl $DMGS"
if [ "$2" = view ]; then
  for n in $names; do
    printf '{"name":"%s","url":"https://github.com/basilelt/overseer/releases/download/v%s/%s"}\n' "$n" "$v" "$n"
  done | jq -s .
else
  for n in $names; do echo "$(printf %s "$n" | sha256sum | cut -d' ' -f1)  $n"; done
fi
SH
chmod +x "$work/bin/gh"

export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ANALYTICS=1
mkdir -p "$tap_dir"
git -C "$tap_dir" init -q
brew trust overseer-ci/tap >/dev/null 2>&1 || true

fail=0
check() {
  local name=$1 dmgs=$2 arch=$3
  local cask="$tap_dir/Casks/overseer.rb" formula="$tap_dir/Formula/overseer.rb"
  rm -rf "${tap_dir:?}/Casks" "${tap_dir:?}/Formula"
  DMGS=$dmgs PATH="$work/bin:$PATH" "$root/scripts/update-tap.sh" 9.8.7 "$tap_dir" >/dev/null
  git -C "$tap_dir" add -A
  git -C "$tap_dir" -c commit.gpgsign=false -c user.name=ci -c user.email=ci@localhost commit -qm "$name"
  if brew style overseer-ci/tap &&
    brew audit --strict --formula overseer-ci/tap/overseer &&
    brew audit --strict --cask overseer-ci/tap/overseer &&
    cask_ok "$cask" "$arch" &&
    formula_ok "$formula"; then
    echo "ok   $name"
  else
    echo "FAIL $name"
    fail=1
  fi
}

formula_ok() {
  local n
  for n in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
    grep -q "releases/download/v9.8.7/overseer-9.8.7-$n\"" "$1" &&
      grep -q "sha256 \"$(printf %s "overseer-9.8.7-$n" | sha256sum | cut -d' ' -f1)\"" "$1" || return 1
  done
}

cask_ok() {
  grep -q 'url "https://github.com/basilelt/overseer/releases/download/v#{version}/' "$1" &&
    grep -q 'strategy :github_latest' "$1" &&
    grep -q '^  auto_updates true$' "$1" &&
    grep -q 'zap launchctl: "dev.overseer.daemon"' "$1" &&
    grep -q 'args: *\["uninstall", "--apply"\]' "$1" &&
    grep -q '"~/.overseer",' "$1" &&
    grep -q '"~/Library/LaunchAgents/dev.overseer.daemon.plist",' "$1" &&
    case $2 in
      both) grep -q 'on_arm do' "$1" && grep -q 'on_intel do' "$1" ;;
      arm) grep -q 'depends_on arch: :arm64' "$1" && ! grep -q 'on_intel do' "$1" ;;
      intel) grep -q 'depends_on arch: :x86_64' "$1" && ! grep -q 'on_arm do' "$1" ;;
    esac
}

check "both dmgs" "Overseer_9.8.7_aarch64.dmg Overseer_9.8.7_x64.dmg" both
check "arm64 dmg only" "Overseer_9.8.7_aarch64.dmg" arm
check "x64 dmg only" "Overseer_9.8.7_x64.dmg" intel
exit "$fail"
