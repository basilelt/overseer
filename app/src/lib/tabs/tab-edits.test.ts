import { afterEach, describe, expect, it, vi } from 'vitest';
import { closeTabAsking, moveMenu, tabClosed, tabMenu } from './tab-edits';
import { EMPTY_LAYOUT, forgetLayouts, layoutFromTabs, update, workspace } from '../workspace/workspace.svelte';
import { DaemonError } from '../daemon/client';

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

describe('tabClosed on a shell that exited', () => {
  const exitedApp = (deleteTerm: () => Promise<unknown>) => {
    const app = {
      items: [{ key: 'term:tm_1', lineId: null, termId: 'tm_1' }],
      data: { terms: [{ id: 'tm_1', exited: true }, { id: 'tm_2', exited: false }] },
      api: { deleteTerm: vi.fn(deleteTerm) },
      notify: vi.fn(),
    };
    return app;
  };

  it('removes the terminal from the daemon and from the board', async () => {
    const app = exitedApp(async () => ({}));
    await tabClosed(app as never, 'term:tm_1', true);
    expect(app.api.deleteTerm).toHaveBeenCalledWith('tm_1');
    expect(app.data.terms.map((t) => t.id)).toEqual(['tm_2']);
  });

  it('sends one delete when the same terminal is closed twice before the first answers', async () => {
    let answer!: () => void;
    const app = exitedApp(() => new Promise((r) => (answer = () => r({}))));
    const first = tabClosed(app as never, 'term:tm_1', true);
    const second = tabClosed(app as never, 'term:tm_1', true);
    answer();
    await Promise.all([first, second]);
    expect(app.api.deleteTerm).toHaveBeenCalledTimes(1);
  });

  it('treats a terminal another window already deleted as gone, without an error', async () => {
    const app = exitedApp(async () => {
      throw new DaemonError(404, 'no term tm_1');
    });
    await tabClosed(app as never, 'term:tm_1', true);
    expect(app.data.terms.map((t) => t.id)).toEqual(['tm_2']);
    expect(app.notify).not.toHaveBeenCalled();
  });

  it('keeps the terminal when the delete fails for another reason', async () => {
    const app = exitedApp(async () => {
      throw new DaemonError(500, 'boom');
    });
    await tabClosed(app as never, 'term:tm_1', true);
    expect(app.data.terms.map((t) => t.id)).toEqual(['tm_1', 'tm_2']);
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

describe('closing a tab by its x, middle click or menu', () => {
  const asking = (items: object[], terms: object[] = []) => ({ items, data: { terms }, open: vi.fn(), canMoveToWorkspace: () => false }) as never as { items: object[]; data: { terms: object[] }; open: ReturnType<typeof vi.fn> };

  afterEach(() => {
    update(() => EMPTY_LAYOUT);
    forgetLayouts();
  });

  it('asks before ending a live session, like Ctrl+W, and keeps its tab', () => {
    update(() => layoutFromTabs(['line:a', 'browser:b'], 'browser:b'));
    const app = asking([{ key: 'line:a', lineId: 'a' }]);
    closeTabAsking(app as never, 0, 'line:a');
    expect(app.open).toHaveBeenCalledWith('closeTab', 'line:a');
    expect(workspace.layout.panes[0].tabs).toEqual(['line:a', 'browser:b']);
    expect(workspace.layout.focused).toBe(0);
  });

  it('closes a tab that holds no live session at once', () => {
    update(() => layoutFromTabs(['browser:a', 'term:t']));
    const app = asking([{ key: 'term:t', termId: 't' }], [{ id: 't', exited: true }]);
    closeTabAsking(app as never, 0, 'browser:a');
    closeTabAsking(app as never, 0, 'term:t');
    expect(app.open).not.toHaveBeenCalled();
    expect(workspace.layout.panes[0].tabs).toEqual([]);
  });

  it('puts the same question behind the Close entry of the tab menu', () => {
    update(() => layoutFromTabs(['term:t']));
    const app = asking([{ key: 'term:t', termId: 't' }], [{ id: 't', exited: false }]);
    const close = tabMenu(app as never, {} as MouseEvent, 0, 'term:t', true).find((m) => m.label === 'Close');
    close!.run!();
    expect(app.open).toHaveBeenCalledWith('closeTab', 'term:t');
  });
});
