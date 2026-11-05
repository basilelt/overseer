# 0006 Deep-link scheme

Status: accepted, implemented for `resume` in 0017

## Decision
`overseer://resume/<lineId>` and `overseer://line/<lineId>` (show) and
`overseer://ticket/<AS-NNNN>` (Board filtered). Registered on macOS by a minimal app bundle
(`Overseer.app`, `CFBundleURLTypes`) that execs `overseer open <url>`; on Linux by a
`.desktop` file with `x-scheme-handler/overseer`. The handler only resolves the id through the
local daemon and then runs the normal `resume` path (which asks for confirmation if the line is
`active` elsewhere). No action is taken from parameters other than the id; unknown paths are rejected.

TwinFactory may also link `https://<api>/v1/lines/<id>` for a read-only view.

## Consequences
One extra macOS artefact to sign/notarize (or ad-hoc for internal use).
