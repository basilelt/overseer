import { describe, expect, it, vi } from 'vitest';
import { moveMenu, tabClosed } from './tab-edits';

function fakeApp(lineId: string | null, termId: string | null, fail?: Error) {
  const close = vi.fn(async () => {
    if (fail) throw fail;
  });
  const request = vi.fn(async (_m: string, _p: string) => {
    if (fail) throw fail;
  });
  const killTerm = vi.fn(async () => {});
  const app = {
    items: [{ key: lineId ? `line:${lineId}` : `term:${termId}`, lineId, termId }],
    data: { terms: [] },
    client: { request },
    api: { close, killTerm, deleteTerm: vi.fn(async () => {}) },
    refresh: vi.fn(async () => {}),
    notify: vi.fn(),
  };
  return { app: app as never, close, request, killTerm, notify: app.notify, refresh: app.refresh };
}

describe('tabClosed on a session', () => {
  it('forces the close so a busy agent is ended with its tab', async () => {
    const { app, request, refresh } = fakeApp('ln_1', null);
    await tabClosed(app, 'line:ln_1', true);
    expect(request).toHaveBeenCalledWith('POST', '/v1/lines/ln_1/close?dryRun=false&force=true');
    expect(refresh).toHaveBeenCalled();
  });

  it('does nothing when the tab is only hidden', async () => {
    const { app, request, killTerm } = fakeApp('ln_1', null);
    await tabClosed(app, 'line:ln_1', false);
    expect(request).not.toHaveBeenCalled();
    expect(killTerm).not.toHaveBeenCalled();
  });

  it('reports a failed close and still refreshes so the session is not hidden from the board', async () => {
    const { app, notify, refresh } = fakeApp('ln_1', null, new Error('agent pid 1 did not exit'));
    await tabClosed(app, 'line:ln_1', true);
    expect(notify).toHaveBeenCalledWith('agent pid 1 did not exit', 'error');
    expect(refresh).toHaveBeenCalled();
  });
});

describe('moveMenu', () => {
  it('names a nested workspace after its parent and checks the current one', () => {
    const w = (id: string, name: string, over = {}) => ({ id, name, path: `/${id}`, rank: 1, pinned: false, archived: false, folderId: null, ...over });
    const app = {
      ws: { workspaces: [w('p', 'endurance'), w('n', 'Onboarding', { parentId: 'p' })], folders: [], groups: [] },
      itemWs: new Map([['line:l', 'p']]),
      moveToWorkspace: vi.fn(async () => true),
    };
    const items = moveMenu(app as never, 'line:l');
    expect(items.map((i) => [i.label, !!i.checked])).toEqual([['endurance', true], ['endurance › Onboarding', false]]);
    items[1]!.run!();
    expect(app.moveToWorkspace).toHaveBeenCalledWith('line:l', 'n');
  });
});
