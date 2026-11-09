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
  local name=$1 dmgs=$2
  rm -rf "${tap_dir:?}/Casks" "${tap_dir:?}/Formula"
  DMGS=$dmgs PATH="$work/bin:$PATH" "$root/scripts/update-tap.sh" 9.8.7 "$tap_dir" >/dev/null
  git -C "$tap_dir" add -A
  git -C "$tap_dir" -c commit.gpgsign=false -c user.name=ci -c user.email=ci@localhost commit -qm "$name"
  if brew style overseer-ci/tap &&
    brew audit --strict --formula overseer-ci/tap/overseer &&
    brew audit --strict --cask overseer-ci/tap/overseer &&
    grep -q 'url "https://github.com/basilelt/overseer/releases/download/v#{version}/' "$tap_dir/Casks/overseer.rb" &&
    grep -q 'strategy :github_latest' "$tap_dir/Casks/overseer.rb" &&
    grep -q '^  auto_updates true$' "$tap_dir/Casks/overseer.rb" &&
    grep -q 'zap launchctl: "dev.overseer.daemon"' "$tap_dir/Casks/overseer.rb" &&
    grep -q 'args:         \["uninstall", "--apply"\]' "$tap_dir/Casks/overseer.rb" &&
    grep -q '"~/.overseer",' "$tap_dir/Casks/overseer.rb" &&
    grep -q '"~/Library/LaunchAgents/dev.overseer.daemon.plist",' "$tap_dir/Casks/overseer.rb"; then
    echo "ok   $name"
  else
    echo "FAIL $name"
    fail=1
  fi
}

check "both dmgs" "Overseer_9.8.7_aarch64.dmg Overseer_9.8.7_x64.dmg"
check "arm64 dmg only" "Overseer_9.8.7_aarch64.dmg"
check "x64 dmg only" "Overseer_9.8.7_x64.dmg"
exit "$fail"
