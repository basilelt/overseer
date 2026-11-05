#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <version> [output]" >&2
  exit 2
fi
version=${1#v}
out=${2:-latest.json}
repo=${OVERSEER_REPO:-basilelt/overseer}

release=$(gh release view "v$version" --repo "$repo" --json assets,body,publishedAt)
sigs=$(mktemp -d)
trap 'rm -rf "$sigs"' EXIT
gh release download "v$version" --repo "$repo" --pattern '*.sig' --dir "$sigs" 2>/dev/null || true

platforms='{}'
add() {
  local key=$1 suffix=$2 name url sig
  name=$(jq -r --arg s "$suffix" '[.assets[].name | select(endswith($s))] | first // empty' <<<"$release")
  if [ -z "$name" ]; then
    echo "warning: v$version has no *$suffix, $key left out" >&2
    return
  fi
  if [ ! -s "$sigs/$name.sig" ]; then
    echo "warning: v$version has no $name.sig, $key left out" >&2
    return
  fi
  url=$(jq -r --arg n "$name" '.assets[] | select(.name == $n) | .url' <<<"$release")
  sig=$(cat "$sigs/$name.sig")
  platforms=$(jq --arg k "$key" --arg u "$url" --arg s "$sig" '.[$k] = {url: $u, signature: $s}' <<<"$platforms")
}

add darwin-aarch64 _aarch64.app.tar.gz
add darwin-x86_64 _x64.app.tar.gz
add windows-x86_64 _x64-setup.exe
add windows-aarch64 _arm64-setup.exe
add linux-x86_64 _amd64.AppImage
add linux-aarch64 _aarch64.AppImage

if [ "$platforms" = '{}' ]; then
  echo "v$version has no signed updater artifact" >&2
  exit 1
fi

jq -n --arg v "$version" --argjson p "$platforms" --argjson r "$release" \
  '{version: $v, notes: ($r.body // ""), pub_date: $r.publishedAt, platforms: $p}' >"$out"
echo "$out: $(jq -r '.platforms | keys | join(", ")' "$out")"
