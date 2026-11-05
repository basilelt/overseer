#!/usr/bin/env bash
set -euo pipefail

dir=$1
shift
[ $# -gt 0 ] || exit 0

sudo=sudo
[ "$(id -u)" = 0 ] && sudo=
mkdir -p "$dir/partial"

# The archive holds the .debs of the lists it was made with; installing from other lists would
# want other versions, so bring those lists back.
if [ -d "$dir/lists" ]; then
  $sudo rm -rf /var/lib/apt/lists
  $sudo cp -a "$dir/lists" /var/lib/apt/lists
fi

# man-db rebuilds its index after every install; nothing in CI reads man pages.
$sudo rm -f /var/lib/man-db/auto-update

opts=(
  -y -q --no-install-recommends
  -o "Dir::Cache::Archives=$dir"
  -o APT::Sandbox::User=root
  -o APT::Keep-Downloaded-Packages=true
  -o Dpkg::Options::=--force-unsafe-io
)
export DEBIAN_FRONTEND=noninteractive

# Lists can be older than the mirror: an install from them only works when every .deb it wants is
# already in the archive. Otherwise refresh the lists and retry.
if ! $sudo apt-get install "${opts[@]}" "$@"; then
  echo "::notice title=apt::archive or package lists stale, running apt-get update"
  $sudo apt-get update -q
  $sudo apt-get install "${opts[@]}" "$@"
fi

$sudo rm -rf "$dir/partial" "$dir/lock" "$dir/lists"
$sudo cp -a /var/lib/apt/lists "$dir/lists"
$sudo rm -rf "$dir/lists/partial" "$dir/lists/lock"
$sudo chown -R "$(id -u):$(id -g)" "$dir"
