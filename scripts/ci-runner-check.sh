#!/bin/sh
set -u
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
[ -n "${GITHUB_PATH:-}" ] && [ -d "$HOME/.cargo/bin" ] && echo "$HOME/.cargo/bin" >>"$GITHUB_PATH"
missing=
for need in "$@"; do
  case $need in
    pkg:*) pkg-config --exists "${need#pkg:}" 2>/dev/null || missing="$missing $need" ;;
    target:*) rustup target list --installed 2>/dev/null | grep -qx "${need#target:}" || missing="$missing $need" ;;
    env:*) eval "test -n \"\${${need#env:}:-}\"" || missing="$missing $need" ;;
    *) command -v "$need" >/dev/null 2>&1 || missing="$missing $need" ;;
  esac
done
if [ -n "$missing" ]; then
  echo "::error title=runner template incomplete::missing on $(hostname):$missing (add a setup step for it, see docs/ci.md)"
  exit 1
fi
echo "runner ok: $*"
