import { afterEach, describe, expect, it, vi } from 'vitest';

const { app } = vi.hoisted(() => ({ app: {} as Record<string, any> }));

vi.mock('../state.svelte', () => ({ app, NO_WORKSPACE: '' }));
vi.mock('../window', () => ({ openWindow: vi.fn(), windowLabel: () => 'main' }));

import { allowed } from './keyboard';
import { allCommands } from './commands';
import { SECTIONS } from './shortcut-sheet';
import { bindings, type ShortcutId } from './shortcuts';

const SCOPE: Record<ShortcutId, 'global' | 'agents-view'> = {
  palette: 'global',
  settings: 'global',
  shortcuts: 'global',
  newTab: 'agents-view',
  closeTab: 'agents-view',
  reopenTab: 'agents-view',
  nextTab: 'agents-view',
  prevTab: 'agents-view',
  tabIndex: 'agents-view',
  splitRight: 'agents-view',
  splitDown: 'agents-view',
  focusLeft: 'agents-view',
  focusRight: 'agents-view',
  focusUp: 'agents-view',
  focusDown: 'agents-view',
  find: 'agents-view',
  copy: 'agents-view',
  paste: 'agents-view',
  fontUp: 'global',
  fontDown: 'global',
  fontReset: 'global',
  toggleSidebar: 'global',
  sourceControl: 'global',
  togglePanel: 'global',
  kanban: 'global',
  newWorkspace: 'global',
  reconnect: 'global',
  newWindow: 'global',
  closeWindow: 'global',
};
const IDS = Object.keys(SCOPE) as ShortcutId[];
const OVERLAY_OK: ShortcutId[] = ['palette', 'settings', 'shortcuts'];
const HIDDEN_FROM_SHEET: ShortcutId[] = [];
const HANDLED_ELSEWHERE: ShortcutId[] = [];

afterEach(() => {
  for (const k of Object.keys(app)) delete app[k];
});

describe('shortcut registry consistency', () => {
  it('binds every shortcut on at least one platform setting', () => {
    const bound = new Set<ShortcutId>();
    for (const mac of [true, false]) for (const ctrlWClosesTab of [true, false]) for (const b of bindings({ mac, ctrlWClosesTab })) bound.add(b.id);
    expect(IDS.filter((id) => !bound.has(id))).toEqual([]);
  });

  it('lists every shortcut in exactly one sheet section unless hidden', () => {
    const listed = SECTIONS.flatMap((s) => s.ids);
    for (const id of IDS) expect(listed.filter((x) => x === id).length, id).toBe(HIDDEN_FROM_SHEET.includes(id) ? 0 : 1);
  });

  it('backs every shortcut with a registry command unless handled elsewhere', () => {
    const wired = new Set(allCommands().map((c) => c.shortcut));
    expect(IDS.filter((id) => !wired.has(id) && !HANDLED_ELSEWHERE.includes(id))).toEqual([]);
  });

  it('scopes every shortcut as the table says', () => {
    for (const id of IDS) {
      Object.assign(app, { overlay: null, view: 'kanban', menu: null });
      expect(allowed(id), `${id} outside agents view`).toBe(SCOPE[id] === 'global');
      Object.assign(app, { overlay: null, view: 'agents', menu: null });
      expect(allowed(id), `${id} in agents view`).toBe(true);
      Object.assign(app, { overlay: 'task' });
      expect(allowed(id), `${id} over an overlay`).toBe(OVERLAY_OK.includes(id));
    }
  });
});
