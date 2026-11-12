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

export function bindings({ mac, ctrlWClosesTab }: ShortcutOptions): Binding[] {
  const m = (key: string, extra: Omit<Combo, 'key' | 'mod'> = {}): Combo => ({ key, mod: true, ...extra });
  const termSafe = (key: string): Combo[] => (mac ? [m(key)] : [m(key, { shift: true }), m(key, { outsideTerminal: true })]);
  const table: Record<ShortcutId, Combo[]> = {
    palette: [m('K'), ...termSafe('P')],
    settings: [m(',')],
    shortcuts: mac ? [m('/', { anyShift: true })] : [m('/', { shift: true }), m('/', { outsideTerminal: true })],
    newTab: [m('T')],
    closeTab: mac || ctrlWClosesTab ? [m('W')] : [],
    reopenTab: [m('T', { shift: true })],
    nextTab: [{ key: 'Tab', ctrl: true }, m(']', { shift: true })],
    prevTab: [{ key: 'Tab', ctrl: true, shift: true }, m('[', { shift: true })],
    tabIndex: [m(DIGITS)],
    splitRight: [mac ? m('D') : m('E', { shift: true })],
    splitDown: [m('D', { shift: true })],
    focusLeft: [m('ArrowLeft', { alt: true })],
    focusRight: [m('ArrowRight', { alt: true })],
    focusUp: [m('ArrowUp', { alt: true })],
    focusDown: [m('ArrowDown', { alt: true })],
    find: termSafe('F'),
    copy: mac ? [m('C', { native: true })] : [m('C', { shift: true, terminalOnly: true }), { key: 'Insert', ctrl: true, terminalOnly: true }],
    paste: mac ? [m('V', { native: true })] : [m('V', { shift: true, terminalOnly: true }), { key: 'Insert', shift: true, terminalOnly: true }],
    fontUp: [m('=', { anyShift: true })],
    fontDown: [m('-', { anyShift: true })],
    fontReset: [m('0')],
    toggleSidebar: termSafe('B'),
    sourceControl: [m('G', { shift: true })],
    togglePanel: termSafe('J'),
    kanban: [m('K', { shift: true })],
    newWorkspace: [m('N', { shift: true })],
    reconnect: termSafe('R'),
    newWindow: [m('N', mac ? { alt: true, native: true } : { alt: true })],
    closeWindow: mac ? [m('W', { shift: true, native: true })] : [],
  };
  return (Object.keys(table) as ShortcutId[]).map((id): Binding => ({ id, combos: table[id] })).filter((b) => b.combos.length);
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

const MODIFIERS = [
  ['ctrl', 'Ctrl', '⌃'],
  ['alt', 'Alt', '⌥'],
  ['shift', 'Shift', '⇧'],
  ['meta', 'Cmd', '⌘'],
] as const;

export function comboModifiers(m: Partial<Record<(typeof MODIFIERS)[number][0], boolean>>, symbols: boolean): string[] {
  return MODIFIERS.filter(([k]) => m[k]).map(([, name, symbol]) => (symbols ? symbol : name));
}

export function formatCombo(c: Combo, mac: boolean): string {
  if (mac) return comboModifiers({ ctrl: c.ctrl, alt: c.alt, shift: c.shift, meta: c.mod }, true).join('') + (MAC_KEYS[c.key] ?? c.key);
  return [...comboModifiers({ ctrl: c.ctrl || c.mod, alt: c.alt, shift: c.shift }, false), OTHER_KEYS[c.key] ?? c.key].join('+');
}

export function shortcutLabel(id: ShortcutId, opts: ShortcutOptions): string {
  const c = bindings(opts).find((b) => b.id === id)?.combos[0];
  return c ? formatCombo(c, opts.mac) : '';
}

export function accelerator(id: ShortcutId): string {
  const c = bindings({ mac: true, ctrlWClosesTab: false }).find((b) => b.id === id)?.combos[0];
  if (!c) return '';
  return ['CmdOrCtrl', c.alt ? 'Alt' : '', c.shift ? 'Shift' : '', c.key.toUpperCase()].filter(Boolean).join('+');
}
