#!/bin/sh
set -eu
out=${1:-}
[ -n "$out" ] || { echo "usage: $0 OUT.aab" >&2; exit 2; }
. "$(dirname "$0")/android-common.sh"
root=$(cd "$(dirname "$0")/.." && pwd)
dir=$root/app/src-tauri/gen/android/app/build/outputs/bundle
find_one "android aab" "release AAB" "$dir" "*-release.aab"
aabs=$found
if ! jarsigner -verify "$aabs" | grep -q 'jar verified'; then
  jarsigner -verify -verbose "$aabs" | tail -20
  echo "::error title=android aab::$aabs is not signed"
  exit 1
fi
certs=$(keytool -printcert -jarfile "$aabs")
printf '%s\n' "$certs" | grep -E 'Owner:|SHA256:'
if [ -n "${ANDROID_CERT_SHA256:-}" ]; then
  want=$(printf '%s' "$ANDROID_CERT_SHA256" | norm_fp)
  if ! printf '%s\n' "$certs" | norm_fp | grep -q "$want"; then
    echo "::error title=android aab::AAB not signed by the expected key ($ANDROID_CERT_SHA256)"
    exit 1
  fi
fi
manifest=$(unzip -p "$aabs" base/manifest/AndroidManifest.xml | tr -c '[:print:]' '\n')
if ! printf '%s\n' "$manifest" | grep -q 'android.permission.INTERNET'; then
  echo "::error title=android aab::cannot read the permissions of $aabs"
  exit 1
fi
if printf '%s\n' "$manifest" | grep -q 'REQUEST_INSTALL_PACKAGES'; then
  echo "::error title=android aab::$aabs declares REQUEST_INSTALL_PACKAGES; Google Play apps may not install updates themselves (build it with OVERSEER_DISTRIBUTION=play)"
  exit 1
fi
if unzip -p "$aabs" 'base/dex/*.dex' | grep -aq 'io/github/basilelt/overseer/ApkInstallerPlugin'; then
  echo "::error title=android aab::$aabs contains ApkInstallerPlugin; the Google Play build must not ship the in-app APK installer"
  exit 1
fi
if printf '%s\n' "$manifest" | grep -q 'androidx.core.content.FileProvider'; then
  echo "::error title=android aab::$aabs declares the APK installer's FileProvider"
  exit 1
fi
publish "$aabs" "$out"
