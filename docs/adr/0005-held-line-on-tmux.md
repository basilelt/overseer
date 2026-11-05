# 0005 Held-line representation on tmux stations

Status: accepted, execution in 0014

## Context
On tmux stations each cmux tab is a persistent tmux session. A held line must keep its tab
meaningful and must stay held after a reboot. Observed: `tmux-snapshot` only records a Claude pane
when a Claude process is found, so a held pane is already restored as a plain shell (held stays
held), but the user loses the context of what was there.

## Options
1. Stub in the same pane: after stopping the agent, the pane runs `overseer stub <lineId>`
   ("held — AS-1234 — press Enter to resume"); Enter execs the resume command in place. Pane option
   `@sb_held=<lineId>`. A toolkit patch makes `tmux-restore` relaunch the stub.
2. Kill the pane, keep only the registry entry; resume opens a new tab.
3. Leave a plain shell with a printed banner; resume by `overseer resume`.

## Decision (recommended)
Option 1. `@sb_held` is a new option, so it never collides with toolkit options. Without the
toolkit patch the behaviour degrades to option 3 after a reboot, which is still safe.

## Consequences
The stub is a tiny foreground process (no agent RAM). The patch to `tmux-snapshot`/`tmux-restore`
is delivered as a diff for the user to apply. Claude background sessions (`claude --bg`) use
`claude stop <id>` instead; no stub.
