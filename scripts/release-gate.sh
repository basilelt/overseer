#!/usr/bin/env bash
set -uo pipefail
. "$(dirname "$0")/docs-paths.sh"

out() { echo "$1=$2" | tee -a "${GITHUB_OUTPUT:-/dev/null}"; }

base=HEAD~1
if [ -n "${BEFORE:-}" ] && [ "${BEFORE//0/}" != "" ] && git cat-file -e "$BEFORE^{commit}" 2>/dev/null; then
  base=$BEFORE
fi

release=false
if ! files=$(git diff --name-only "$base" HEAD); then
  echo "::warning title=release gate::git diff $base HEAD failed, releasing"
  release=true
else
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    is_docs_path "$f" || { release=true; break; }
  done <<<"$files"
fi
out release "$release"
if [ "$release" != true ]; then
  echo "::notice title=release gate::docs-only push, no release"
  out skip_ci false
  exit 0
fi

why() { echo "::notice title=release gate::running CI: $1"; out skip_ci false; exit 0; }

pr_head=$(git rev-parse -q --verify 'HEAD^2^{commit}') || why "not a merge commit"
git rev-parse -q --verify 'HEAD^3' >/dev/null && why "octopus merge"
git merge-base --is-ancestor HEAD^1 "$pr_head" || why "the PR branch was behind main"
git diff --quiet "$pr_head" HEAD || why "the merged tree differs from the PR head"

prs=$(gh api "repos/$GITHUB_REPOSITORY/commits/$pr_head/pulls" --jq 'length') || why "PR lookup failed"
[ "${prs:-0}" -ge 1 ] || why "$pr_head is not the head of a PR"

runs=$(gh api --paginate "repos/$GITHUB_REPOSITORY/commits/$pr_head/check-runs?per_page=100" \
  --jq '.check_runs[] | [.name, .status, (.conclusion // "none")] | @tsv') || why "check-runs lookup failed"
[ -n "$runs" ] || why "no check runs on $pr_head"
echo "$runs"
bad=$(awk -F'\t' '$2 != "completed" || ($3 != "success" && $3 != "skipped" && $3 != "neutral")' <<<"$runs")
[ -z "$bad" ] || why "check runs not green on $pr_head: $(tr '\t\n' ' ;' <<<"$bad")"
# A draft PR run skips every job, "changes" included: its success proves a non-draft PR run.
awk -F'\t' '$1 == "changes" && $3 == "success" {found=1} END {exit !found}' <<<"$runs" \
  || why "no successful non-draft PR CI run on $pr_head"

echo "::notice title=release gate::$pr_head passed CI as a PR and equals the merge, CI not repeated"
out skip_ci true
