# Regenerating the showreel

The showreel in this directory (live at https://basilelt.github.io/overseer/, the repository's website) shows the app as it ships. Remaking it is expensive: the scenes in `reel.html` have to be updated by hand, then both cuts take about 25 minutes of rendering on a 6-core machine. So an agent never regenerates it on its own judgement: it asks the user, and only a clear yes starts the work.

## When to ask

Ask when your branch, once merged, leaves the reel showing something wrong or missing something big:

- a major new feature or view, the kind that gets its own entry under "Features" in `README.md`;
- a supported agent added or removed (the reel names every agent);
- a platform added or dropped (desktop, phone, tablet, server over SSH);
- a fundamental design change: the logo, the color palette or theme tokens, the typography, or the layout of the main window.

Do not ask for bug fixes, small UI tweaks, copy changes, refactors, tests, CI or docs.

Ask once per branch, as the last step of the work, before the PR is marked ready. Never ask in the middle of a task, and never ask again on a branch where the user already answered.

## How to ask

Ask one yes/no question, with No as the default, and say what it costs:

> This branch [one line on what changed] leaves the showreel out of date. Regenerating it means updating the scenes and about 25 minutes of rendering. Regenerate the showreel now? (yes / no, default no)

Use your harness's question tool when it has one (in Claude Code, AskUserQuestion with "No" as the first option); otherwise ask in plain text and stop there.

## The answer

- **Yes**: regenerate it, following the steps below.
- **No**: do not regenerate it. Write "Showreel: not regenerated (the user said no)" in the PR description and finish your task.
- **No answer**: treat it as no. This covers a non-interactive run where you cannot ask, a reply about something else, and no reply within 10 minutes when your harness lets you wait with a timeout. Write "Showreel: not regenerated (no answer)" in the PR description and finish your task.

Do not start a render, or any other part of the work, while you wait for the answer.

## Regenerating

1. Update the scenes in `reel.html` to show the app as it now is. Keep every frame a pure function of time on the 128 BPM grid, and both cuts in sync (`reel.html?d=30` plays every event at twice its time). If a view changed, copy the new screenshot from `app/docs/screenshots/` into `shots/`.
2. If cue times moved, move the matching events in `audio.mjs` and rebuild both soundtracks.
3. Check stills or a contact sheet at every scene change of both cuts, then render both cuts and the poster (commands in [README.md](README.md)).
4. Commit this directory, videos included, on the branch, so the change lands on `next` with the PR. After the merge, publish it to `gh-pages` (README.md, "Publish").
