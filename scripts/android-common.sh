norm_fp() { tr -d ': ' | tr 'A-F' 'a-f'; }

find_one() {
  found=$(find "$3" -type f -name "$4" 2>/dev/null)
  count=$(printf '%s' "$found" | grep -c . || true)
  if [ "$count" -ne 1 ]; then
    echo "::error title=$1::expected one $2 under $3, found $count"
    printf '%s\n' "$found"
    exit 1
  fi
}

publish() {
  mkdir -p "$(dirname "$2")"
  cp "$1" "$2"
  (cd "$(dirname "$2")" && sha256sum "$(basename "$2")" >"$(basename "$2").sha256")
  ls -l "$2"
}
