import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { app } from '../state.svelte';
import { ui } from './ui.svelte';
import { drawerPick } from './drawer';

const before = { key: app.selectedKey, ws: app.workspaceId, drawer: ui.drawer };
beforeEach(() => {
  ui.drawer = true;
});
afterEach(() => {
  app.selectedKey = before.key;
  app.workspaceId = before.ws;
  ui.drawer = before.drawer;
});

function click(handler: () => void) {
  drawerPick.begin();
  handler();
  drawerPick.end();
}

describe('drawer pick', () => {
  it('closes when the already selected session is picked again', () => {
    app.selectedKey = 'line:ln_03';
    click(() => app.select('line:ln_03'));
    expect(ui.drawer).toBe(false);
  });

  it('closes when the already selected workspace is picked again', () => {
    click(() => app.selectWorkspace(app.workspaceId));
    expect(ui.drawer).toBe(false);
  });

  it('stays open on a click that picks nothing, like a folding chevron', () => {
    click(() => {});
    expect(ui.drawer).toBe(true);
  });

  it('stays open when a selection happens outside a click in the drawer', () => {
    drawerPick.begin();
    drawerPick.end();
    app.select('line:ln_03');
    expect(ui.drawer).toBe(true);
  });
});

describe('drawer pick action', () => {
  const target = () => {
    const calls: [string, boolean | undefined][] = [];
    const handlers: Record<string, (e: Event) => void> = {};
    const add = (who: string) => (t: string, f: (e: Event) => void, c?: boolean) => {
      calls.push([`${who} add ${t}`, c]);
      handlers[`${who}${c ? ' capture' : ''}`] = f;
    };
    const remove = (who: string) => (t: string, _f: unknown, c?: boolean) => calls.push([`${who} remove ${t}`, c]);
    return { calls, handlers, add, remove };
  };

  afterEach(() => vi.unstubAllGlobals());

  it('marks in capture on the drawer and checks on the window, after the delegated handlers of the app root', () => {
    const t = target();
    vi.stubGlobal('window', { addEventListener: t.add('window'), removeEventListener: t.remove('window') });
    const node = { addEventListener: t.add('node'), removeEventListener: t.remove('node') } as unknown as HTMLElement;
    const action = drawerPick.watch(node);
    const tap = new Event('click');
    t.handlers['node capture'](tap);
    app.select(app.selectedKey);
    t.handlers['window'](tap);
    expect(ui.drawer).toBe(false);
    action.destroy();
    expect(t.calls).toEqual([['node add click', true], ['window add click', undefined], ['node remove click', true], ['window remove click', undefined]]);
  });

  it('ignores a click outside the drawer even when it selects', () => {
    const t = target();
    vi.stubGlobal('window', { addEventListener: t.add('window'), removeEventListener: t.remove('window') });
    drawerPick.watch({ addEventListener: t.add('node'), removeEventListener: t.remove('node') } as unknown as HTMLElement);
    t.handlers['node capture'](new Event('click'));
    app.select(app.selectedKey);
    t.handlers['window'](new Event('click'));
    expect(ui.drawer).toBe(true);
  });
});
