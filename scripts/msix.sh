#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 4 ]; then
  echo "usage: $0 <version> <x64|arm64> <app.exe> <out.msix>   (env MSIX_PFX_BASE64, MSIX_PFX_PASSWORD)" >&2
  exit 2
fi
version=${1#v}
arch=$2
exe=$3
out=$4
: "${MSIX_PFX_BASE64:?MSIX_PFX_BASE64 is empty}"
: "${MSIX_PFX_PASSWORD:?MSIX_PFX_PASSWORD is empty}"
publisher=${MSIX_PUBLISHER:-CN=basilelt}
root=$(cd "$(dirname "$0")/.." && pwd)

case $(uname -m) in
  aarch64 | arm64) host=arm64 ;;
  *) host=x64 ;;
esac
kits="/c/Program Files (x86)/Windows Kits/10/bin"
sdk=$(find "$kits" -maxdepth 1 -type d -name '10.*' | sort -V | tail -1)
makeappx="$sdk/$host/makeappx.exe"
signtool="$sdk/$host/signtool.exe"
for t in "$makeappx" "$signtool"; do
  [ -x "$t" ] || { echo "::error title=MSIX::missing $t" >&2; exit 1; }
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage=$work/stage
mkdir -p "$stage/assets"
cp "$exe" "$stage/Overseer.exe"
if [ -d "$root/app/src-tauri/resources/server" ]; then
  cp -R "$root/app/src-tauri/resources/server" "$stage/server"
fi
icons=$root/app/src-tauri/icons
cp "$icons/128x128@2x.png" "$stage/assets/Square150x150Logo.png"
cp "$icons/128x128.png" "$stage/assets/Square44x44Logo.png"
cp "$icons/128x128.png" "$stage/assets/StoreLogo.png"
sed -e "s|@VERSION@|$version|" -e "s|@ARCH@|$arch|" -e "s|@PUBLISHER@|$publisher|" \
  "$root/packaging/msix/AppxManifest.xml" >"$stage/AppxManifest.xml"

pfx=$work/sign.pfx
printf '%s' "$MSIX_PFX_BASE64" | base64 --decode >"$pfx"
mkdir -p "$(dirname "$out")"
MSYS2_ARG_CONV_EXCL='*' "$makeappx" pack /o /d "$(cygpath -w "$stage")" /p "$(cygpath -w "$out")"
MSYS2_ARG_CONV_EXCL='*' "$signtool" sign /fd SHA256 /f "$(cygpath -w "$pfx")" /p "$MSIX_PFX_PASSWORD" "$(cygpath -w "$out")"
openssl pkcs12 -in "$pfx" -nokeys -clcerts -passin env:MSIX_PFX_PASSWORD |
  openssl x509 -outform der -out "$(dirname "$out")/overseer-msix.cer"
echo "$out signed for $publisher"
