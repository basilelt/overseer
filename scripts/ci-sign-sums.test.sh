#!/bin/sh
set -u

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

check() {
  if [ "$2" = "$3" ]; then
    echo "ok   $1"
  else
    echo "FAIL $1: want '$3', got '$2'"
    fail=1
  fi
}

assets() {
  rm -rf "$work/assets"
  mkdir -p "$work/assets"
  printf 'one' >"$work/assets/a.bin"
  printf 'two' >"$work/assets/b c.bin"
  printf 'stale' >"$work/assets/SHA256SUMS"
  printf 'stale' >"$work/assets/SHA256SUMS.sig"
}

cat >"$work/sign-ok" <<'SH'
#!/bin/sh
printf 'c2ln' >"$1.sig"
SH
cat >"$work/sign-nothing" <<'SH'
#!/bin/sh
exit 0
SH
cat >"$work/verify-ok" <<'SH'
#!/bin/sh
exit 0
SH
cat >"$work/verify-bad" <<'SH'
#!/bin/sh
exit 1
SH
chmod +x "$work"/sign-* "$work"/verify-*
pub=$(printf 'pub' | base64)

assets
out=$(TAURI_SIGNING_PRIVATE_KEY='' "$here/ci-sign-sums.sh" "$work/assets" 2>&1)
check "no key: succeeds" "$?" "0"
check "no key: warns" "$(printf '%s' "$out" | grep -c '::warning title=checksums not signed::')" "1"
check "no key: no signature, stale one removed" "$(find "$work/assets" -type f -exec basename {} \; | LC_ALL=C sort | tr '\n' ' ')" "SHA256SUMS a.bin b c.bin "
check "the checksums list every asset and not themselves" "$(cut -c67- "$work/assets/SHA256SUMS" | tr '\n' '|')" "a.bin|b c.bin|"
check "the checksums verify" "$(cd "$work/assets" && sha256sum -c --quiet SHA256SUMS 2>&1; echo $?)" "0"

assets
out=$(TAURI_SIGNING_PRIVATE_KEY=k SIGN_SUMS_SIGNER="$work/sign-ok" SIGN_SUMS_PUBKEY="$pub" MINISIGN="$work/verify-ok" "$here/ci-sign-sums.sh" "$work/assets" 2>&1)
check "key: succeeds" "$?:$out" "0:signed 2 checksums"
check "key: signature kept" "$(cat "$work/assets/SHA256SUMS.sig")" "c2ln"

assets
out=$(TAURI_SIGNING_PRIVATE_KEY=k SIGN_SUMS_SIGNER="$work/sign-nothing" SIGN_SUMS_PUBKEY="$pub" MINISIGN="$work/verify-ok" "$here/ci-sign-sums.sh" "$work/assets" 2>&1)
check "a signer that writes nothing fails the release" "$?:$out" "1:::error title=checksums not signed::the signer wrote no SHA256SUMS.sig"

assets
out=$(TAURI_SIGNING_PRIVATE_KEY=k SIGN_SUMS_SIGNER="$work/sign-ok" SIGN_SUMS_PUBKEY="$pub" MINISIGN="$work/verify-bad" "$here/ci-sign-sums.sh" "$work/assets" 2>&1)
check "a signature that does not verify fails the release" "$?:$(printf '%s' "$out" | grep -c '::error title=checksums signature invalid::')" "1:1"
check "a signature that does not verify is not published" "$(test -e "$work/assets/SHA256SUMS.sig"; echo $?)" "1"

tauri=${SIGN_SUMS_TEST_TAURI:-$here/../app/node_modules/.bin/tauri}
minisign=${MINISIGN:-minisign}
if [ -x "$tauri" ] && command -v "$minisign" >/dev/null 2>&1; then
  "$tauri" signer generate --ci -p '' -w "$work/key" >/dev/null 2>&1
  "$tauri" signer generate --ci -p '' -w "$work/other" >/dev/null 2>&1
  cat >"$work/sign-real" <<SH
#!/bin/sh
exec "$tauri" signer sign "\$1"
SH
  chmod +x "$work/sign-real"
  assets
  out=$(TAURI_SIGNING_PRIVATE_KEY=$(cat "$work/key") TAURI_SIGNING_PRIVATE_KEY_PASSWORD='' SIGN_SUMS_SIGNER="$work/sign-real" SIGN_SUMS_PUBKEY=$(cat "$work/key.pub") MINISIGN="$minisign" "$here/ci-sign-sums.sh" "$work/assets" 2>&1)
  check "tauri signer and minisign: signed and verified" "$?:$out" "0:signed 2 checksums"
  assets
  out=$(TAURI_SIGNING_PRIVATE_KEY=$(cat "$work/key") TAURI_SIGNING_PRIVATE_KEY_PASSWORD='' SIGN_SUMS_SIGNER="$work/sign-real" SIGN_SUMS_PUBKEY=$(cat "$work/other.pub") MINISIGN="$minisign" "$here/ci-sign-sums.sh" "$work/assets" 2>&1)
  check "tauri signer and minisign: another public key is refused" "$?" "1"
else
  echo "skip tauri signer and minisign round trip: one of them is not installed"
fi

exit $fail
