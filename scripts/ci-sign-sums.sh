#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <assets-dir>" >&2
  exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
dir=$(cd "$1" && pwd)
sums=$dir/SHA256SUMS
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rm -f "$sums" "$sums.sig"
(cd "$dir" && sha256sum -- * >"$work/sums")
mv "$work/sums" "$sums"

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "::warning title=checksums not signed::TAURI_SIGNING_PRIVATE_KEY is not set: no SHA256SUMS.sig, so the phone app refuses to install the server of this release on a host (docs/ci.md)"
  exit 0
fi

if [ -n "${SIGN_SUMS_SIGNER:-}" ]; then
  "$SIGN_SUMS_SIGNER" "$sums" >/dev/null
else
  pnpm --dir "$root/app" exec tauri signer sign "$sums" >/dev/null
fi
if [ ! -s "$sums.sig" ]; then
  echo "::error title=checksums not signed::the signer wrote no SHA256SUMS.sig"
  exit 1
fi

pubkey=${SIGN_SUMS_PUBKEY:-$(jq -r '.plugins.updater.pubkey' "$root/app/src-tauri/tauri.conf.json")}
printf '%s' "$pubkey" | base64 -d >"$work/pub"
base64 -d "$sums.sig" >"$work/sig"
if ! "${MINISIGN:-minisign}" -Vm "$sums" -p "$work/pub" -x "$work/sig" >/dev/null; then
  rm -f "$sums.sig"
  echo "::error title=checksums signature invalid::SHA256SUMS.sig does not verify with plugins.updater.pubkey of app/src-tauri/tauri.conf.json: TAURI_SIGNING_PRIVATE_KEY is not its private key"
  exit 1
fi
echo "signed $(wc -l <"$sums") checksums"
