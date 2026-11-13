#!/bin/sh
set -eu
kind=${1:-}
out=${2:-}
case $kind in
  release | debug) ;;
  *) echo "usage: $0 release|debug OUT.apk" >&2; exit 2 ;;
esac
[ -n "$out" ] || { echo "usage: $0 release|debug OUT.apk" >&2; exit 2; }
. "$(dirname "$0")/android-common.sh"
root=$(cd "$(dirname "$0")/.." && pwd)
dir=$root/app/src-tauri/gen/android/app/build/outputs/apk
find_one "android apk" "$kind APK" "$dir" "*-$kind*.apk"
apks=$found
case $apks in
  *unsigned*) echo "::error title=android apk::$apks is unsigned"; exit 1 ;;
esac
apksigner=$(command -v apksigner || ls -d "$ANDROID_HOME"/build-tools/*/apksigner 2>/dev/null | sort -V | tail -1)
certs=$("$apksigner" verify --verbose --print-certs "$apks" 2>&1)
printf '%s\n' "$certs" | grep -iE 'verified using|certificate'
if [ -n "${ANDROID_CERT_SHA256:-}" ]; then
  want=$(printf '%s' "$ANDROID_CERT_SHA256" | norm_fp)
  if ! printf '%s' "$want" | grep -qE '^[0-9a-f]{64}$'; then
    echo "::error title=android apk::ANDROID_CERT_SHA256 is not a SHA-256 fingerprint"
    exit 1
  fi
  if ! printf '%s\n' "$certs" | norm_fp | grep -q "$want"; then
    printf '%s\n' "$certs"
    echo "::error title=android apk::APK not signed by the expected key ($ANDROID_CERT_SHA256)"
    exit 1
  fi
fi
aapt2=$(command -v aapt2 || ls -d "$ANDROID_HOME"/build-tools/*/aapt2 2>/dev/null | sort -V | tail -1)
if ! "$aapt2" dump permissions "$apks" | grep -q 'android.permission.REQUEST_INSTALL_PACKAGES'; then
  echo "::error title=android apk::$apks lacks REQUEST_INSTALL_PACKAGES, so its in-app updater cannot install updates (was it built with OVERSEER_DISTRIBUTION=play?)"
  exit 1
fi
if ! unzip -p "$apks" 'classes*.dex' | grep -aq 'io/github/basilelt/overseer/ApkInstallerPlugin'; then
  echo "::error title=android apk::$apks does not contain ApkInstallerPlugin, so its in-app updater cannot install updates"
  exit 1
fi
publish "$apks" "$out"
