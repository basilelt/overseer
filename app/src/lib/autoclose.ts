import type { app as appState } from './state.svelte';
import type { Term } from './types';
import { reapEnded } from './tab-edits';
import { activeKey, closeTabAt, workspace } from './workspace.svelte';
import { isShellId } from './agents';

export function cleanExits(prev: Term[], next: Term[]): string[] {
  const was = new Map(prev.map((t) => [t.id, t.exited]));
  return next.filter((t) => t.exited && was.get(t.id) === false && !t.lineId && !t.taskId && (!t.agent || isShellId(t.agent)) && [0, 130].includes(t.exitCode ?? 0)).map((t) => t.id);
}

export function closeCleanExits(app: typeof appState, prev: Term[], next: Term[]) {
  reapEnded(app, next);
  for (const id of cleanExits(prev, next)) {
    const key = `term:${id}`;
    const selected = app.selectedKey === key;
    for (let i = workspace.layout.panes.length - 1; i >= 0; i--) closeTabAt(i, key);
    if (selected) app.selectedKey = activeKey(workspace.layout);
  }
}
