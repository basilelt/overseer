#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <out-dir>   (env MSIX_PFX_PASSWORD, optional MSIX_PUBLISHER, default CN=basilelt)" >&2
  exit 2
fi
: "${MSIX_PFX_PASSWORD:?MSIX_PFX_PASSWORD is empty}"
out=$1
subject=${MSIX_PUBLISHER:-CN=basilelt}
mkdir -p "$out"
umask 077
openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \
  -subj "/CN=${subject#CN=}" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=codeSigning" \
  -keyout "$out/overseer-msix.key" -out "$out/overseer-msix.pem" 2>/dev/null
openssl pkcs12 -export -inkey "$out/overseer-msix.key" -in "$out/overseer-msix.pem" \
  -passout env:MSIX_PFX_PASSWORD -out "$out/overseer-msix.pfx"
openssl x509 -in "$out/overseer-msix.pem" -outform der -out "$out/overseer-msix.cer"
rm -f "$out/overseer-msix.key" "$out/overseer-msix.pem"
base64 <"$out/overseer-msix.pfx" | tr -d '\n' >"$out/overseer-msix.pfx.b64"
echo "secret WINDOWS_SELF_SIGN_PFX_BASE64: $out/overseer-msix.pfx.b64"
echo "secret WINDOWS_SELF_SIGN_PFX_PASSWORD: the MSIX_PFX_PASSWORD you used"
echo "keep $out/overseer-msix.pfx private; the release publishes the public certificate"
