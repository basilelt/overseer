import { afterEach, beforeEach, describe, expect, it } from 'vitest';
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
  it('listens in capture then bubble and removes both on destroy', () => {
    const calls: [string, boolean | undefined][] = [];
    const node = {
      addEventListener: (t: string, _f: unknown, c?: boolean) => calls.push([`add ${t}`, c]),
      removeEventListener: (t: string, _f: unknown, c?: boolean) => calls.push([`remove ${t}`, c]),
    } as unknown as HTMLElement;
    drawerPick.watch(node).destroy();
    expect(calls).toEqual([['add click', true], ['add click', undefined], ['remove click', true], ['remove click', undefined]]);
  });
});
