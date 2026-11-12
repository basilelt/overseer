import { beforeEach, describe, expect, it } from 'vitest';
import { DaemonError, setClient } from '../daemon/client';
import { LOCAL_TARGET } from '../conn';
import { MockDaemon } from '../daemon/mock';
import { app, tabLazy } from '../state.svelte';
import type { LocalLine } from '../types';
import { itemMenu } from '../workspace/wsactions';

class Flaky extends MockDaemon {
  refuse = false;

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    if (this.refuse && method === 'PUT' && path.endsWith('/title')) throw new DaemonError(500, 'disk full');
    return super.request<T>(method, path, body);
  }
}

const itemOf = (key: string) => app.items.find((i) => i.key === key);

describe('renameItem', () => {
  let m: Flaky;

  beforeEach(async () => {
    m = new Flaky({ live: false, delayMs: 0 });
    setClient(m);
    app.client = null;
    app.toast = null;
    await app.connect(LOCAL_TARGET);
    // The tab bar preloads the rename code when idle; the app is past that point here.
    await tabLazy();
  });

  it('shows the custom title at once and keeps it once the daemon stores it', async () => {
    // Given a connected app showing a session line
    // When the user renames it
    // Then the cleaned title shows before the daemon answers and is stored by the daemon
    const key = app.items.find((i) => i.lineId)!.key;
    const done = app.renameItem(key, '  Payments  ');
    expect(itemOf(key)).toMatchObject({ title: 'Payments', renamed: true });
    await done;
    const lines = await m.request<LocalLine[]>('GET', '/v1/lines');
    expect(lines.find((l) => `line:${l.line.id}` === key)?.local?.titleOverride).toBe('Payments');
    expect(itemOf(key)?.title).toBe('Payments');
  });

  it('restores the previous title and reports the error when the daemon refuses', async () => {
    // Given a session line and a daemon that refuses renames
    // When the user renames it
    // Then the previous title comes back and the error is shown
    const item = app.items.find((i) => i.lineId)!;
    m.refuse = true;
    await app.renameItem(item.key, 'Payments');
    expect(itemOf(item.key)?.title).toBe(item.title);
    expect(app.toast).toMatchObject({ kind: 'error', text: expect.stringContaining('disk full') });
  });

  it('clears a custom title back to the auto title', async () => {
    // Given a renamed session line
    // When the user clears its title
    // Then the auto title shows again
    const item = app.items.find((i) => i.lineId)!;
    await app.renameItem(item.key, 'Payments');
    await app.renameItem(item.key, null);
    expect(itemOf(item.key)?.title).toBe(item.title);
    expect(itemOf(item.key)?.renamed).toBeFalsy();
  });

  it('offers rename in the item menu, and reset only once renamed', async () => {
    // Given a session line with its auto title
    // When its menu is opened before and after a rename, and reset is run
    // Then rename is always offered, reset only when renamed, and reset restores the auto title
    const item = app.items.find((i) => i.lineId)!;
    const labels = () => itemMenu(item.key).map((m) => m.label);
    expect(labels()).toContain('Rename…');
    expect(labels()).not.toContain('Reset title');
    await app.renameItem(item.key, 'Payments');
    const reset = itemMenu(item.key).find((m) => m.label === 'Reset title');
    reset!.run!();
    await Promise.resolve();
    expect(itemOf(item.key)?.title).toBe(item.title);
  });

  it('renames a plain terminal tab', async () => {
    // Given a loose terminal
    // When the user renames it
    // Then the terminal shows the custom title
    const key = app.items.find((i) => i.key.startsWith('term:'))!.key;
    await app.renameItem(key, 'build watch');
    expect(itemOf(key)).toMatchObject({ title: 'build watch', renamed: true });
  });
});
