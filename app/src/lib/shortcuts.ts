export type ShortcutId =
  | 'palette'
  | 'settings'
  | 'shortcuts'
  | 'newTab'
  | 'closeTab'
  | 'reopenTab'
  | 'nextTab'
  | 'prevTab'
  | 'tabIndex'
  | 'splitRight'
  | 'splitDown'
  | 'focusLeft'
  | 'focusRight'
  | 'focusUp'
  | 'focusDown'
  | 'find'
  | 'copy'
  | 'paste'
  | 'fontUp'
  | 'fontDown'
  | 'fontReset'
  | 'toggleSidebar'
  | 'sourceControl'
  | 'togglePanel'
  | 'kanban'
  | 'newWorkspace'
  | 'reconnect'
  | 'newWindow'
  | 'closeWindow';

export interface Combo {
  key: string;
  mod?: boolean;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  anyShift?: boolean;
  outsideTerminal?: boolean;
  terminalOnly?: boolean;
  native?: boolean;
}

export interface Binding {
  id: ShortcutId;
  combos: Combo[];
}

export interface ShortcutOptions {
  mac: boolean;
  ctrlWClosesTab: boolean;
}

export interface KeyEventLike {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export const DIGITS = '1…9';

export const SECTIONS: { id: 'tabs' | 'panes' | 'terminal' | 'app'; ids: ShortcutId[] }[] = [
  { id: 'tabs', ids: ['newTab', 'closeTab', 'reopenTab', 'nextTab', 'prevTab', 'tabIndex'] },
  { id: 'panes', ids: ['splitRight', 'splitDown', 'focusLeft', 'focusRight', 'focusUp', 'focusDown'] },
  { id: 'terminal', ids: ['find', 'copy', 'paste', 'fontUp', 'fontDown', 'fontReset'] },
  { id: 'app', ids: ['palette', 'settings', 'shortcuts', 'toggleSidebar', 'sourceControl', 'togglePanel', 'kanban', 'newWorkspace', 'reconnect', 'newWindow', 'closeWindow'] },
];

export function bindings({ mac, ctrlWClosesTab }: ShortcutOptions): Binding[] {
  const m = (key: string, extra: Omit<Combo, 'key' | 'mod'> = {}): Combo => ({ key, mod: true, ...extra });
  const termSafe = (key: string): Combo[] => (mac ? [m(key)] : [m(key, { shift: true }), m(key, { outsideTerminal: true })]);
  const list: Binding[] = [
    { id: 'palette', combos: [m('K'), ...termSafe('P')] },
    { id: 'settings', combos: [m(',')] },
    { id: 'shortcuts', combos: mac ? [m('/', { anyShift: true })] : [m('/', { shift: true }), m('/', { outsideTerminal: true })] },
    { id: 'newTab', combos: [m('T')] },
    { id: 'closeTab', combos: mac || ctrlWClosesTab ? [m('W')] : [] },
    { id: 'reopenTab', combos: [m('T', { shift: true })] },
    { id: 'nextTab', combos: [{ key: 'Tab', ctrl: true }, m(']', { shift: true })] },
    { id: 'prevTab', combos: [{ key: 'Tab', ctrl: true, shift: true }, m('[', { shift: true })] },
    { id: 'tabIndex', combos: [m(DIGITS)] },
    { id: 'splitRight', combos: [mac ? m('D') : m('E', { shift: true })] },
    { id: 'splitDown', combos: [m('D', { shift: true })] },
    { id: 'focusLeft', combos: [m('ArrowLeft', { alt: true })] },
    { id: 'focusRight', combos: [m('ArrowRight', { alt: true })] },
    { id: 'focusUp', combos: [m('ArrowUp', { alt: true })] },
    { id: 'focusDown', combos: [m('ArrowDown', { alt: true })] },
    { id: 'find', combos: termSafe('F') },
    { id: 'copy', combos: mac ? [m('C', { native: true })] : [m('C', { shift: true, terminalOnly: true }), { key: 'Insert', ctrl: true, terminalOnly: true }] },
    { id: 'paste', combos: mac ? [m('V', { native: true })] : [m('V', { shift: true, terminalOnly: true }), { key: 'Insert', shift: true, terminalOnly: true }] },
    { id: 'fontUp', combos: [m('=', { anyShift: true })] },
    { id: 'fontDown', combos: [m('-', { anyShift: true })] },
    { id: 'fontReset', combos: [m('0')] },
    { id: 'toggleSidebar', combos: termSafe('B') },
    { id: 'sourceControl', combos: [m('G', { shift: true })] },
    { id: 'togglePanel', combos: termSafe('J') },
    { id: 'kanban', combos: [m('K', { shift: true })] },
    { id: 'newWorkspace', combos: [m('N', { shift: true })] },
    { id: 'reconnect', combos: termSafe('R') },
    { id: 'newWindow', combos: [m('N', mac ? { alt: true, native: true } : { alt: true })] },
    { id: 'closeWindow', combos: mac ? [m('W', { shift: true, native: true })] : [] },
  ];
  return list.filter((b) => b.combos.length);
}

const ALIASES: Record<string, string> = { '+': '=', _: '-', '{': '[', '}': ']', '?': '/', '<': ',' };
const PUNCT = new Set(['=', '-', '[', ']', '/', ',']);

export function eventKey(e: Pick<KeyEventLike, 'key' | 'code'>): string {
  const k = ALIASES[e.key] ?? e.key;
  if (PUNCT.has(k)) return k;
  if (/^Digit[0-9]$/.test(e.code)) return e.code.slice(5);
  if (/^Numpad[0-9]$/.test(e.code)) return e.code.slice(6);
  if (e.code === 'NumpadAdd') return '=';
  if (e.code === 'NumpadSubtract') return '-';
  if (/^[a-z]$/i.test(k)) return k.toUpperCase();
  if (/^Key[A-Z]$/.test(e.code) && k.length === 1) return e.code.slice(3);
  return k;
}

function matches(c: Combo, e: KeyEventLike, mac: boolean, key: string): boolean {
  if (c.ctrl) {
    if (!e.ctrlKey || (mac && e.metaKey)) return false;
  } else if (c.mod) {
    const primary = mac ? e.metaKey : e.ctrlKey;
    const other = mac ? e.ctrlKey : e.metaKey;
    if (!primary || other) return false;
  } else if (e.ctrlKey || e.metaKey) return false;
  if (!!c.alt !== e.altKey) return false;
  if (!c.anyShift && !!c.shift !== e.shiftKey) return false;
  if (c.key === DIGITS) return /^[1-9]$/.test(key);
  return c.key === key;
}

export interface Resolved {
  id: ShortcutId;
  index?: number;
}

export function resolveShortcut(e: KeyEventLike, opts: ShortcutOptions & { inTerminal: boolean }): Resolved | null {
  const key = eventKey(e);
  if (!e.metaKey && !e.ctrlKey && key !== 'Insert') return null;
  for (const b of bindings(opts)) {
    for (const c of b.combos) {
      if (c.native) continue;
      if (c.outsideTerminal && opts.inTerminal) continue;
      if (c.terminalOnly && !opts.inTerminal) continue;
      if (!matches(c, e, opts.mac, key)) continue;
      return b.id === 'tabIndex' ? { id: b.id, index: Number(key) - 1 } : { id: b.id };
    }
  }
  return null;
}

const MAC_KEYS: Record<string, string> = { ArrowLeft: '←', ArrowRight: '→', ArrowUp: '↑', ArrowDown: '↓', Tab: '⇥' };
const OTHER_KEYS: Record<string, string> = { ArrowLeft: 'Left', ArrowRight: 'Right', ArrowUp: 'Up', ArrowDown: 'Down' };

export function formatCombo(c: Combo, mac: boolean): string {
  if (mac) {
    const mods = (c.ctrl ? '⌃' : '') + (c.alt ? '⌥' : '') + (c.shift ? '⇧' : '') + (c.mod ? '⌘' : '');
    return mods + (MAC_KEYS[c.key] ?? c.key);
  }
  const parts = [c.ctrl || c.mod ? 'Ctrl' : '', c.alt ? 'Alt' : '', c.shift ? 'Shift' : '', OTHER_KEYS[c.key] ?? c.key].filter(Boolean);
  return parts.join('+');
}

export function shortcutLabel(id: ShortcutId, opts: ShortcutOptions): string {
  const c = bindings(opts).find((b) => b.id === id)?.combos[0];
  return c ? formatCombo(c, opts.mac) : '';
}

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
