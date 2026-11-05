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

dmg_arm=Overseer_${version}_aarch64.dmg
dmg_intel=Overseer_${version}_x64.dmg
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

has_asset() {
  jq -e --arg n "$1" 'any(.[]; .name == $n)' <<<"$assets" >/dev/null
}

cask_url() {
  local v='#{version}'
  echo "${url[$1]//$version/$v}"
}

cask_meta() {
  cat <<RUBY
  name "Overseer"
  desc "Desktop client for Overseer agent sessions"
  homepage "https://github.com/$repo"
RUBY
}

cask_arch() {
  local arm=$1 intel=$2
  if [ -n "$arm" ] && [ -n "$intel" ]; then
    cat <<RUBY

  on_arm do
    sha256 "${sum[$arm]}"

    url "$(cask_url "$arm")"
  end
  on_intel do
    sha256 "${sum[$intel]}"

    url "$(cask_url "$intel")"
  end

$(cask_meta)
RUBY
  else
    local d=${arm:-$intel}
    cat <<RUBY
  sha256 "${sum[$d]}"

  url "$(cask_url "$d")"
$(cask_meta)
RUBY
  fi
}

cask_depends() {
  if [ -n "$1" ] && [ -n "$2" ]; then
    echo "  depends_on :macos"
  elif [ -n "$1" ]; then
    printf '  depends_on arch: :arm64\n  depends_on :macos\n'
  else
    printf '  depends_on arch: :x86_64\n  depends_on :macos\n'
  fi
}

dmgs=()
for d in "$dmg_arm" "$dmg_intel"; do
  if has_asset "$d"; then
    url[$d]=$(asset_url "$d")
    sum[$d]=$(sha "$d")
    dmgs+=("$d")
  else
    echo "warning: release v$version has no $d" >&2
    [ "$d" = "$dmg_arm" ] && dmg_arm= || dmg_intel=
  fi
done

if [ ${#dmgs[@]} -gt 0 ]; then
  cat >"$tap/Casks/overseer.rb" <<RUBY
cask "overseer" do
  version "$version"
$(cask_arch "$dmg_arm" "$dmg_intel")

$(cask_depends "$dmg_arm" "$dmg_intel")

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
  echo "warning: release v$version has no .dmg, Casks/overseer.rb left unchanged" >&2
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
