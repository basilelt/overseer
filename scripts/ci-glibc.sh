#!/usr/bin/env bash
set -uo pipefail

max=${1:?usage: ci-glibc.sh <max glibc, e.g. 2.35> <file or dir>...}
shift
fail=0
while IFS= read -r -d '' f; do
  head -c4 "$f" 2>/dev/null | grep -q $'^\x7fELF' || continue
  need=$(objdump -T "$f" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
  [ -n "$need" ] || continue
  if [ "$(printf '%s\n%s\n' "$need" "$max" | sort -V | tail -1)" != "$max" ]; then
    echo "::error title=glibc too new::${f} needs GLIBC_${need}, newer than ${max}"
    fail=1
  fi
done < <(find "$@" -type f -print0)
[ "$fail" = 0 ] && echo "glibc ok: every ELF needs at most GLIBC_${max}"
exit "$fail"
