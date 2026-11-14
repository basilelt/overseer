import { bindings, formatCombo, type Binding, type ShortcutId, type ShortcutOptions } from './shortcuts';

export const SECTIONS: { id: 'tabs' | 'panes' | 'terminal' | 'app'; ids: ShortcutId[] }[] = [
  { id: 'tabs', ids: ['newTab', 'closeTab', 'reopenTab', 'nextTab', 'prevTab', 'tabIndex'] },
  { id: 'panes', ids: ['splitRight', 'splitDown', 'focusLeft', 'focusRight', 'focusUp', 'focusDown'] },
  { id: 'terminal', ids: ['find', 'copy', 'paste', 'fontUp', 'fontDown', 'fontReset'] },
  { id: 'app', ids: ['palette', 'settings', 'shortcuts', 'toggleSidebar', 'sourceControl', 'togglePanel', 'kanban', 'newWorkspace', 'reconnect', 'newWindow', 'closeWindow'] },
];

export interface SheetRow {
  id: ShortcutId;
  keys: { label: string; outsideTerminal: boolean }[];
}

export function sheetRows(opts: ShortcutOptions): { id: (typeof SECTIONS)[number]['id']; rows: SheetRow[] }[] {
  const all = bindings(opts);
  return SECTIONS.map((s) => ({
    id: s.id,
    rows: s.ids
      .map((id) => all.find((b) => b.id === id))
      .filter((b): b is Binding => !!b)
      .map((b) => ({ id: b.id, keys: b.combos.map((c) => ({ label: formatCombo(c, opts.mac), outsideTerminal: !!c.outsideTerminal })) })),
  }));
}
