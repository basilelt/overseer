#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <version> <tap-dir>" >&2
  exit 2
fi
version=${1#v}
tap=$2
repo=${OVERSEER_REPO:-basilelt/overseer}

assets=$(gh release view "v$version" --repo "$repo" --json assets --jq '.assets')
sums=$(gh release download "v$version" --repo "$repo" --pattern SHA256SUMS --output -)

asset_url() {
  local u
  u=$(jq -r --arg n "$1" '.[] | select(.name == $n) | .url' <<<"$assets")
  if [ -z "$u" ]; then
    echo "release v$version has no asset $1" >&2
    exit 1
  fi
  echo "$u"
}

sha() {
  local s
  s=$(awk -v n="$1" '$2 == n || $2 == "*" n { print $1 }' <<<"$sums")
  if [ -z "$s" ]; then
    echo "SHA256SUMS of v$version has no entry for $1" >&2
    exit 1
  fi
  echo "$s"
}

dmg=Overseer_${version}_aarch64.dmg
mac_arm=overseer-$version-aarch64-apple-darwin
mac_intel=overseer-$version-x86_64-apple-darwin
linux_arm=overseer-$version-aarch64-unknown-linux-musl
linux_intel=overseer-$version-x86_64-unknown-linux-musl

declare -A url sum
for n in "$mac_arm" "$mac_intel" "$linux_arm" "$linux_intel"; do
  url[$n]=$(asset_url "$n")
  sum[$n]=$(sha "$n")
done

mkdir -p "$tap/Casks" "$tap/Formula"

if jq -e --arg n "$dmg" 'any(.[]; .name == $n)' <<<"$assets" >/dev/null; then
  url[$dmg]=$(asset_url "$dmg")
  sum[$dmg]=$(sha "$dmg")
  v='#{version}'
  cat >"$tap/Casks/overseer.rb" <<RUBY
cask "overseer" do
  version "$version"
  sha256 "${sum[$dmg]}"

  url "${url[$dmg]//$version/$v}"
  name "Overseer"
  desc "Desktop client for Overseer agent sessions"
  homepage "https://github.com/$repo"

  depends_on arch: :arm64
  depends_on :macos

  app "Overseer.app"

  zap trash: [
    "~/Library/Application Support/io.github.basilelt.overseer",
    "~/Library/Caches/io.github.basilelt.overseer",
    "~/Library/Preferences/io.github.basilelt.overseer.plist",
    "~/Library/Saved Application State/io.github.basilelt.overseer.savedState",
    "~/Library/WebKit/io.github.basilelt.overseer",
  ]
end
RUBY
else
  echo "warning: release v$version has no $dmg, Casks/overseer.rb left unchanged" >&2
fi

formula_url() {
  cat <<RUBY
      url "${url[$1]}"
      sha256 "${sum[$1]}"
RUBY
}

cat >"$tap/Formula/overseer.rb" <<RUBY
class Overseer < Formula
  desc "Daemon and CLI that supervise coding agent sessions"
  homepage "https://github.com/$repo"

  on_macos do
    on_arm do
$(formula_url "$mac_arm")
    end
    on_intel do
$(formula_url "$mac_intel")
    end
  end

  on_linux do
    on_arm do
$(formula_url "$linux_arm")
    end
    on_intel do
$(formula_url "$linux_intel")
    end
  end

  def install
    bin.install Dir["overseer-*"].first => "overseer"
  end

  test do
    assert_match "\"version\":\"#{version}\"", shell_output("#{bin}/overseer version --json")
  end
end
RUBY

echo "overseer $version written to $tap"
