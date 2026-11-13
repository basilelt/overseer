#!/bin/sh
set -eu
. "$(dirname "$0")/platforms.sh"
version=$("$(dirname "$0")/version.sh")
mkdir -p dist
built=0
for t in ${DIST_TARGETS:-$LINUX_X64 $LINUX_ARM64}; do
  arch=${t%%-*}
  var=$(echo "$t" | tr 'a-z-' 'A-Z_')
  if ! rustup target list --installed | grep -qx "$t"; then
    echo "dist: skip $t (rustup target add $t)" >&2
    continue
  fi
  cc=$(command -v "$arch-linux-musl-gcc" || true)
  [ -z "$cc" ] && [ "$arch" = "$(uname -m)" ] && cc=$(command -v musl-gcc || true)
  cflags=
  if [ -z "$cc" ] && [ "$arch" = "$(uname -m)" ]; then
    cc=$(command -v gcc || command -v cc)
    cflags=-DSQLITE_DISABLE_LFS
  fi
  if [ -z "$cc" ]; then
    echo "dist: skip $t (needs $arch-linux-musl-gcc)" >&2
    continue
  fi
  env "CC_$(echo "$t" | tr - _)=$cc" "CFLAGS_$(echo "$t" | tr - _)=$cflags" \
    "CARGO_TARGET_${var}_LINKER=${LINKER:-$cc}" \
    cargo build --release --target "$t"
  cp "target/$t/release/overseer" "dist/overseer-$version-$t"
  echo "dist/overseer-$version-$t"
  built=$((built + 1))
done
[ "$built" -gt 0 ]
