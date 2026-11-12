import { keyToInput } from '../commands/keys';

export type AccessoryKey = 'esc' | 'tab' | 'ctrl' | 'alt' | 'up' | 'down' | 'left' | 'right' | 'pipe' | 'tilde' | 'slash' | 'dash' | 'newline';

export type ModState = 'off' | 'once' | 'locked';

export interface Mods {
  ctrl: ModState;
  alt: ModState;
}

export const NO_MODS: Mods = { ctrl: 'off', alt: 'off' };

export const ACCESSORY_KEYS: AccessoryKey[] = ['esc', 'tab', 'ctrl', 'alt', 'up', 'down', 'left', 'right', 'pipe', 'tilde', 'slash', 'dash', 'newline'];

export const LABELS: Record<AccessoryKey, string> = {
  esc: 'Esc',
  tab: 'Tab',
  ctrl: 'Ctrl',
  alt: 'Alt',
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  pipe: '|',
  tilde: '~',
  slash: '/',
  dash: '-',
  newline: '⏎',
};

const SHIFT_ENTER = keyToInput({ key: 'Enter', shiftKey: true, ctrlKey: false, altKey: false, metaKey: false }, { shiftEnterNewline: true }) ?? '\r';

const ARROWS: Record<string, string> = { up: 'A', down: 'B', right: 'C', left: 'D' };
const CHARS: Record<string, string> = { pipe: '|', tilde: '~', slash: '/', dash: '-' };

export function nextModState(s: ModState, doubleTap: boolean): ModState {
  if (s === 'locked') return 'off';
  if (doubleTap) return 'locked';
  return s === 'once' ? 'off' : 'once';
}

function consume(m: Mods): Mods {
  return { ctrl: m.ctrl === 'once' ? 'off' : m.ctrl, alt: m.alt === 'once' ? 'off' : m.alt };
}

export function ctrlChar(ch: string): string | null {
  if (ch.length !== 1) return null;
  const c = ch.toUpperCase().charCodeAt(0);
  if (c >= 64 && c <= 95) return String.fromCharCode(c - 64);
  if (ch === ' ' || ch === '2') return '\x00';
  if (ch === '/' || ch === '-' || ch === '7') return '\x1f';
  if (ch === '?' || ch === '8') return '\x7f';
  return null;
}

function arrow(letter: string, m: Mods): string {
  const ctrl = m.ctrl !== 'off';
  const alt = m.alt !== 'off';
  if (!ctrl && !alt) return `\x1b[${letter}`;
  const code = 1 + (alt ? 2 : 0) + (ctrl ? 4 : 0);
  return `\x1b[1;${code}${letter}`;
}

export interface Press {
  send: string | null;
  mods: Mods;
}

export function pressKey(key: AccessoryKey, mods: Mods, doubleTap = false): Press {
  if (key === 'ctrl') return { send: null, mods: { ...mods, ctrl: nextModState(mods.ctrl, doubleTap) } };
  if (key === 'alt') return { send: null, mods: { ...mods, alt: nextModState(mods.alt, doubleTap) } };
  if (key in ARROWS) return { send: arrow(ARROWS[key], mods), mods: consume(mods) };
  if (key === 'esc') return { send: '\x1b', mods: consume(mods) };
  if (key === 'tab') return { send: mods.alt !== 'off' ? '\x1b\t' : '\t', mods: consume(mods) };
  if (key === 'newline') return { send: SHIFT_ENTER, mods: consume(mods) };
  return applyMods(CHARS[key], mods);
}

export function applyMods(text: string, mods: Mods): Press {
  if (!text || (mods.ctrl === 'off' && mods.alt === 'off')) return { send: text, mods };
  const [first, rest] = [text.slice(0, 1), text.slice(1)];
  let out = first;
  if (mods.ctrl !== 'off') out = ctrlChar(first) ?? first;
  if (mods.alt !== 'off') out = `\x1b${out}`;
  return { send: out + rest, mods: consume(mods) };
}
