# 0020 `overseer patch`

Status: accepted

## Context
ADR 0007 decision 2: a developer takes over an agent's line by SSH attach by default, or with a
new local line on demand. A Claude session can only be resumed on the station that holds its
files, and the API never stores conversation content.

## Decision
- `overseer patch AS-NNNN` picks the most recent open line of the ticket, local or through the
  API, preferring lines not owned by the caller (the agent's line).
- Default: attach, exactly `resume <line> --attach` (local tmux attach, or
  `ssh -t <target> -- overseer resume <id> --attach` for another station). `--print` shows it.
- `--local`: a fresh worktree of the same branch at `<worktreeDir>/<repo>/<branch slug>-patch`
  (branch fetched from the local clone's origin), a new line (`--session-id`, reserved in the
  registry, owner = this station's owner, origin `dev`) in tmux session `sb-patch-<branch slug>`,
  seeded with a summary: line id, station, repo, branch, ticket, PR URL (via `gh`) and the last 5
  `ai-title` titles of the previous agent. Titles are read from the transcript locally, or with
  `ssh -T <target> -- overseer titles <id>` (titles only) for another station. Never raw
  conversation content. The summary goes through a `0600` temp file pasted into the agent, then
  deleted; it is not logged.

## Consequences
- The local line starts with metadata, not the agent's reasoning; the developer reads the branch.
- A branch already checked out in another local worktree cannot be patched locally (git refuses);
  attach instead.
