#!/bin/sh
set -eu
[ $# -gt 0 ] || { echo "usage: $0 APP.apk|APP.aab..." >&2; exit 2; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
for pkg in "$@"; do
  [ -f "$pkg" ] || { echo "::error title=16 KB pages::$pkg not found"; exit 1; }
  dir=$work/$(basename "$pkg")
  mkdir -p "$dir"
  unzip -qo "$pkg" 'lib/arm64-v8a/*.so' 'lib/x86_64/*.so' '*/lib/arm64-v8a/*.so' '*/lib/x86_64/*.so' -d "$dir" 2>/dev/null || true
  libs=$(find "$dir" -type f -name '*.so' | sort)
  if [ -z "$libs" ]; then
    echo "::error title=16 KB pages::$pkg has no 64-bit native library"
    fail=1
    continue
  fi
  for so in $libs; do
    name=${so#"$dir"/}
    aligns=$(readelf -lW "$so" | awk '$1 == "LOAD" { print $NF }')
    [ -n "$aligns" ] || { echo "::error title=16 KB pages::$name has no LOAD segment"; fail=1; continue; }
    for a in $aligns; do
      if [ $((a)) -lt 16384 ]; then
        echo "::error title=16 KB pages::$pkg: $name has a LOAD segment aligned to $a, Google Play needs 0x4000 (16 KB)"
        fail=1
        continue 2
      fi
    done
    echo "16 KB ok: $pkg $name"
  done
  case $pkg in
    *.apk)
      if ! zipalign -c -P 16 4 "$pkg" >/dev/null; then
        echo "::error title=16 KB pages::$pkg: uncompressed native libraries are not zip-aligned to 16 KB"
        fail=1
      fi
      ;;
  esac
done
exit "$fail"
