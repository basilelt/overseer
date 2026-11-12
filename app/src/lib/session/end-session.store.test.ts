import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setClient } from '../daemon/client';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import { allTabs, closeTabAt, EMPTY_LAYOUT, workspace } from '../workspace/workspace.svelte';

describe('ending a shell from its tab', () => {
  let m: MockDaemon;
  let id: string;
  let key: string;

  beforeEach(async () => {
    m = new MockDaemon({ live: false, delayMs: 0 });
    setClient(m);
    (app as unknown as { client: unknown }).client = null;
    await app.connect({ kind: 'local', label: 'Local' });
    id = app.data.terms.find((t) => !t.lineId && !t.taskId && !t.exited)!.id;
    key = `term:${id}`;
    workspace.layout = { ...EMPTY_LAYOUT, panes: [{ tabs: [key], active: key }] };
    app.selectedKey = key;
  });

  afterEach(async () => {
    await app.disconnect();
    workspace.layout = EMPTY_LAYOUT;
    app.selectedKey = null;
  });

  const hasTerm = () => app.data.terms.some((t) => t.id === id);

  it('removes the killed term so it cannot come back as a tab', async () => {
    const request = vi.spyOn(m, 'request');
    closeTabAt(0, key, true);
    app.selectedKey = null;
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith('DELETE', `/v1/terms/${encodeURIComponent(id)}`), { timeout: 1000, interval: 5 });
    await vi.waitFor(() => expect(hasTerm()).toBe(false), { timeout: 1000, interval: 5 });
    expect(allTabs(workspace.layout)).toEqual([]);
    expect(app.items.some((i) => i.key === key)).toBe(false);
  });

  it('keeps the term running when the tab is only hidden', async () => {
    const request = vi.spyOn(m, 'request');
    closeTabAt(0, key, false);
    await new Promise((r) => setTimeout(r, 20));
    expect(request).not.toHaveBeenCalledWith('POST', expect.stringContaining('/kill'), expect.anything());
    expect(app.data.terms.find((t) => t.id === id)?.exited).toBe(false);
  });

  it('answers the daemon on DELETE: 404 unknown, 409 running, 200 once exited', async () => {
    await expect(m.request('DELETE', '/v1/terms/nope')).rejects.toMatchObject({ status: 404 });
    await expect(m.request('DELETE', `/v1/terms/${id}`)).rejects.toMatchObject({ status: 409 });
    await m.request('POST', `/v1/terms/${id}/kill`, { confirm: true });
    await expect(m.request('DELETE', `/v1/terms/${id}`)).resolves.toEqual({ deleted: id });
    expect((await m.request<{ id: string }[]>('GET', '/v1/terms')).some((t) => t.id === id)).toBe(false);
  });

  it('leaves the tab and the term alone when the kill is refused', async () => {
    vi.spyOn(m, 'request').mockImplementation(async (method, path) => {
      if (method === 'POST' && path.endsWith('/kill')) throw new Error('boom');
      return [] as never;
    });
    const notify = vi.spyOn(app, 'notify');
    closeTabAt(0, key, true);
    await vi.waitFor(() => expect(notify.mock.calls[0]?.slice(0, 2)).toEqual(['boom', 'error']), { timeout: 1000, interval: 5 });
    expect(hasTerm()).toBe(true);
  });
});
