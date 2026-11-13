is_docs_path() {
  case $1 in
    docs/* | *.md | .github/ISSUE_TEMPLATE/*) return 0 ;;
  esac
  return 1
}
