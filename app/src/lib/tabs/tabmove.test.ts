import { beforeEach, describe, expect, it } from 'vitest';
import { DaemonError, setClient } from '../daemon/client';
import { LOCAL_TARGET } from '../conn';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import { allTabs, EMPTY_LAYOUT, openInFocused, workspace } from '../workspace/workspace.svelte';

class Flaky extends MockDaemon {
  fail = false;

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    if (this.fail && method === 'PUT' && path.endsWith('/workspace')) throw new DaemonError(500, 'boom');
    return super.request<T>(method, path, body);
  }
}

describe('moving a session to another workspace', () => {
  let m: Flaky;

  beforeEach(async () => {
    m = new Flaky({ live: false, delayMs: 0 });
    setClient(m);
    await app.init();
    await app.connect(LOCAL_TARGET);
  });

  function setup(pick: (key: string) => boolean) {
    const item = app.items.find((i) => pick(i.key) && app.itemWs.has(i.key))!;
    const from = app.itemWs.get(item.key)!;
    const to = app.ws.workspaces.find((w) => w.id !== from && !w.archived && !w.missing)!;
    app.selectWorkspace(from);
    workspace.layout = openInFocused(EMPTY_LAYOUT, item.key);
    return { key: item.key, from, to: to.id };
  }

  it('moves a session line to the target workspace and out of the current tab strip', async () => {
    const { key, to } = setup((k) => k.startsWith('line:'));
    expect(await app.moveToWorkspace(key, to)).toBe(true);
    expect(app.itemWs.get(key)).toBe(to);
    expect(app.membersOf(to).some((i) => i.key === key)).toBe(true);
    expect(allTabs(workspace.layout)).not.toContain(key);
    const id = app.items.find((i) => i.key === key)!.lineId!;
    expect((await app.api.lines()).find((l) => l.line.id === id)?.local?.workspaceId).toBe(to);
  });

  it('puts the session back when the daemon refuses the move', async () => {
    const { key, from, to } = setup((k) => k.startsWith('term:'));
    m.fail = true;
    expect(await app.moveToWorkspace(key, to)).toBe(false);
    expect(app.itemWs.get(key)).toBe(from);
    expect(allTabs(workspace.layout)).toContain(key);
  });

  it('refuses tabs that are not sessions and drops on the current workspace', async () => {
    const { key, from } = setup((k) => k.startsWith('line:'));
    expect(app.canMoveToWorkspace('browser:https://x.dev')).toBe(false);
    expect(app.canMoveToWorkspace(key)).toBe(true);
    expect(await app.moveToWorkspace(key, from)).toBe(false);
    expect(allTabs(workspace.layout)).toContain(key);
  });
});
