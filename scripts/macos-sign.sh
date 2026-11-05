#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 import            (env SIGN_P12_BASE64, SIGN_P12_PASSWORD; prints the identity hash)" >&2
  echo "       $0 verify <app> [selfsigned|developerid]" >&2
  echo "       $0 sign-resources <dir> <identity>   (Developer ID + hardened runtime on every Mach-O under dir)" >&2
  exit 2
}

fail() {
  echo "::error title=macOS signing::$*" >&2
  exit 1
}

import() {
  : "${SIGN_P12_BASE64:?SIGN_P12_BASE64 is empty}"
  : "${SIGN_P12_PASSWORD:?SIGN_P12_PASSWORD is empty}"
  local dir kc kcpw p12 cert identity
  dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/sign.XXXXXX")
  kc="$dir/overseer-sign.keychain-db"
  p12="$dir/identity.p12"
  cert="$dir/identity.pem"
  kcpw=$(openssl rand -hex 24)
  printf '%s' "$SIGN_P12_BASE64" | base64 --decode >"$p12"

  security create-keychain -p "$kcpw" "$kc"
  security set-keychain-settings -lut 21600 "$kc"
  security unlock-keychain -p "$kcpw" "$kc"
  security import "$p12" -k "$kc" -P "$SIGN_P12_PASSWORD" -f pkcs12 -T /usr/bin/codesign -T /usr/bin/security >/dev/null
  rm -f "$p12"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kcpw" "$kc" >/dev/null
  local existing=()
  while IFS= read -r k; do
    k=${k//\"/}
    k=${k#"${k%%[![:space:]]*}"}
    [ -n "$k" ] && existing+=("$k")
  done < <(security list-keychains -d user)
  security list-keychains -d user -s "$kc" "${existing[@]}"

  # An Apple-issued identity must chain to Apple's root: a local trustRoot override would break "anchor apple generic".
  if [ "${SIGN_TRUST:-1}" = 1 ]; then
    security find-certificate -a -p "$kc" >"$cert"
    if ! sudo -n security add-trusted-cert -d -r trustRoot -p codeSign -k /Library/Keychains/System.keychain "$cert" >&2; then
      echo "::warning title=macOS signing::could not mark the certificate trusted for code signing; codesign may still accept it" >&2
    fi
  fi

  identity=$(security find-identity -v -p codesigning "$kc" | awk '/^ *[0-9]+\) [0-9A-F]{40} /{print $2; exit}')
  if [ -z "$identity" ]; then
    security find-identity -p codesigning "$kc" >&2 || true
    identity=$(security find-identity -p codesigning "$kc" | awk '/^ *[0-9]+\) [0-9A-F]{40} /{print $2; exit}')
  fi
  [ -n "$identity" ] || fail "no code signing identity in the imported p12"
  if [ -n "${GITHUB_ENV:-}" ]; then
    echo "SIGN_KEYCHAIN=$kc" >>"$GITHUB_ENV"
  fi
  echo "$identity"
}

sign_resources() {
  local dir=${1:-} identity=${2:-} f n=0
  if [ ! -d "$dir" ] || [ -z "$identity" ]; then usage; fi
  while IFS= read -r -d '' f; do
    if file -b "$f" | grep -q '^Mach-O'; then
      codesign --force --options runtime --timestamp --sign "$identity" "$f" || fail "codesign failed for $f"
      n=$((n + 1))
    fi
  done < <(find "$dir" -type f -print0)
  echo "signed $n Mach-O file(s) under $dir"
}

verify() {
  local app=${1:-} kind=${2:-selfsigned} info req
  [ -d "$app" ] || fail "$app is not an app bundle"
  codesign --verify --verbose=2 "$app" || fail "codesign --verify failed for $app"
  info=$(codesign -dv --verbose=4 "$app" 2>&1)
  req=$(codesign -d -r- "$app" 2>&1)
  printf '%s\n' "$info" "$req"
  if grep -q '^Signature=adhoc' <<<"$info"; then
    fail "$app is ad-hoc signed"
  fi
  if grep -q 'cdhash' <<<"$req"; then
    fail "the designated requirement of $app pins a cdhash: it changes with every build"
  fi
  case "$kind" in
    selfsigned)
      grep -q '^Authority=Overseer Self-Signed' <<<"$info" || fail "$app is not signed by Overseer Self-Signed"
      grep -Eq 'certificate (leaf|root) = H"[0-9a-f]{40}"' <<<"$req" || fail "the designated requirement of $app does not pin the certificate"
      ;;
    developerid)
      grep -q '^Authority=Developer ID Application' <<<"$info" || fail "$app is not signed with a Developer ID"
      xcrun stapler validate "$app" || fail "$app has no stapled notarization ticket"
      gk=$(spctl --assess --type execute --verbose=4 "$app" 2>&1) || fail "Gatekeeper rejects $app: $gk"
      echo "$gk"
      grep -q 'source=Notarized Developer ID' <<<"$gk" || fail "Gatekeeper does not accept $app as notarized"
      ;;
    *) usage ;;
  esac
  echo "designated requirement pins the signing certificate: stable across builds"
}

case "${1:-}" in
  import) import ;;
  verify) shift; verify "$@" ;;
  sign-resources) shift; sign_resources "$@" ;;
  *) usage ;;
esac
