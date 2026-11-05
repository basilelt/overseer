#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
image=${CI_APT_TEST_IMAGE:-ubuntu:24.04}
pkgs="tmux xvfb pkg-config libssl-dev"
work=$(mktemp -d)
trap 'docker run --rm -v "$work:/w" "$image" rm -rf /w/archive; rm -rf "$work"' EXIT

run() {
  docker run --rm "$@" -v "$root/scripts:/s:ro" -v "$work:/w" "$image" bash -c '
    set -e
    /s/ci-apt.sh /w/archive '"$pkgs"' >&2
    for c in tmux Xvfb pkg-config; do command -v $c >/dev/null; done
    pkg-config --exists openssl
    test -d /w/archive/lists
    test ! -e /w/archive/partial
    ls /w/archive/*.deb | wc -l'
}

fail=0
cold=$(run) || { echo "FAIL cold install"; exit 1; }
echo "ok   cold install with network filled the archive ($cold debs)"

if warm=$(run --network none); then
  echo "ok   warm install from the archive without network ($warm debs)"
else
  echo "FAIL warm install needed the network"
  fail=1
fi

docker run --rm -v "$work:/w" "$image" sh -c 'rm /w/archive/tmux_*.deb'
if run --network none >/dev/null 2>&1; then
  echo "FAIL install succeeded offline with a .deb missing"
  fail=1
else
  echo "ok   missing .deb falls back to apt-get update (fails offline)"
fi

if run >/dev/null && ls "$work"/archive/tmux_*.deb >/dev/null; then
  echo "ok   fallback refetches the missing .deb into the archive"
else
  echo "FAIL fallback did not refill the archive"
  fail=1
fi

if docker run --rm -v "$root/scripts:/s:ro" "$image" /s/ci-apt.sh /tmp/a; then
  echo "ok   no packages is a no-op"
else
  echo "FAIL no packages"
  fail=1
fi

exit $fail
