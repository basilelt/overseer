#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1
