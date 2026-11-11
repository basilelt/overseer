import { describe, expect, it } from 'vitest';
import { eventKey, formatCombo, resolveShortcut, sheetRows, shortcutLabel, type KeyEventLike } from './shortcuts';

const ev = (key: string, code: string, mods: { meta?: boolean; ctrl?: boolean; shift?: boolean; alt?: boolean } = {}): KeyEventLike => ({
  key,
  code,
  metaKey: !!mods.meta,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
});

const mac = { mac: true, ctrlWClosesTab: true, inTerminal: false };
const linux = { mac: false, ctrlWClosesTab: true, inTerminal: false };
const linuxTerm = { ...linux, inTerminal: true };

describe('shortcut resolver', () => {
  it('uses Cmd on macOS and Ctrl elsewhere', () => {
    expect(resolveShortcut(ev('w', 'KeyW', { meta: true }), mac)).toEqual({ id: 'closeTab' });
    expect(resolveShortcut(ev('w', 'KeyW', { ctrl: true }), mac)).toBeNull();
    expect(resolveShortcut(ev('w', 'KeyW', { ctrl: true }), linux)).toEqual({ id: 'closeTab' });
    expect(resolveShortcut(ev('w', 'KeyW', { meta: true }), linux)).toBeNull();
    expect(resolveShortcut(ev('t', 'KeyT', { ctrl: true }), linuxTerm)).toEqual({ id: 'newTab' });
    expect(resolveShortcut(ev('T', 'KeyT', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'reopenTab' });
    expect(resolveShortcut(ev('T', 'KeyT', { meta: true, shift: true }), mac)).toEqual({ id: 'reopenTab' });
    expect(resolveShortcut(ev('k', 'KeyK', { meta: true }), mac)).toEqual({ id: 'palette' });
    expect(resolveShortcut(ev('K', 'KeyK', { meta: true, shift: true }), mac)).toEqual({ id: 'kanban' });
    expect(resolveShortcut(ev('N', 'KeyN', { ctrl: true, shift: true }), linux)).toEqual({ id: 'newWorkspace' });
    expect(resolveShortcut(ev(',', 'Comma', { ctrl: true }), linux)).toEqual({ id: 'settings' });
  });

  it('opens the palette with Cmd+P, Ctrl+P outside a terminal and Ctrl+Shift+P inside, leaving shell Ctrl+P alone', () => {
    expect(resolveShortcut(ev('p', 'KeyP', { meta: true }), mac)).toEqual({ id: 'palette' });
    expect(resolveShortcut(ev('p', 'KeyP', { ctrl: true }), linux)).toEqual({ id: 'palette' });
    expect(resolveShortcut(ev('p', 'KeyP', { ctrl: true }), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('P', 'KeyP', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'palette' });
  });

  it('reconnects with Cmd+R, Ctrl+R outside a terminal and Ctrl+Shift+R inside, leaving shell Ctrl+R alone', () => {
    expect(resolveShortcut(ev('r', 'KeyR', { meta: true }), mac)).toEqual({ id: 'reconnect' });
    expect(resolveShortcut(ev('r', 'KeyR', { ctrl: true }), linux)).toEqual({ id: 'reconnect' });
    expect(resolveShortcut(ev('r', 'KeyR', { ctrl: true }), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('R', 'KeyR', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'reconnect' });
  });

  it('opens a new window with Ctrl+Alt+N, leaving Cmd+Option+N to the macOS menu', () => {
    expect(resolveShortcut(ev('n', 'KeyN', { ctrl: true, alt: true }), linux)).toEqual({ id: 'newWindow' });
    expect(resolveShortcut(ev('n', 'KeyN', { ctrl: true, alt: true }), linuxTerm)).toEqual({ id: 'newWindow' });
    expect(resolveShortcut(ev('Dead', 'KeyN', { meta: true, alt: true }), mac)).toBeNull();
    expect(shortcutLabel('newWindow', { mac: true, ctrlWClosesTab: true })).toBe('⌥⌘N');
    expect(shortcutLabel('newWindow', { mac: false, ctrlWClosesTab: true })).toBe('Ctrl+Alt+N');
  });

  it('ignores keys without Cmd or Ctrl', () => {
    expect(resolveShortcut(ev('w', 'KeyW'), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('1', 'Digit1'), linux)).toBeNull();
    expect(resolveShortcut(ev('w', 'KeyW', { alt: true }), linux)).toBeNull();
  });

  it('lets Ctrl+W through to the terminal when the setting is off', () => {
    const off = { ...linuxTerm, ctrlWClosesTab: false };
    expect(resolveShortcut(ev('w', 'KeyW', { ctrl: true }), off)).toBeNull();
    expect(resolveShortcut(ev('w', 'KeyW', { meta: true }), { ...mac, ctrlWClosesTab: false })).toEqual({ id: 'closeTab' });
  });

  it('cycles tabs with Ctrl+Tab on every platform and Mod+Shift+brackets', () => {
    expect(resolveShortcut(ev('Tab', 'Tab', { ctrl: true }), mac)).toEqual({ id: 'nextTab' });
    expect(resolveShortcut(ev('Tab', 'Tab', { ctrl: true, shift: true }), mac)).toEqual({ id: 'prevTab' });
    expect(resolveShortcut(ev('Tab', 'Tab', { ctrl: true }), linuxTerm)).toEqual({ id: 'nextTab' });
    expect(resolveShortcut(ev('Tab', 'Tab', { ctrl: true, shift: true }), linux)).toEqual({ id: 'prevTab' });
    expect(resolveShortcut(ev('{', 'BracketLeft', { ctrl: true, shift: true }), linux)).toEqual({ id: 'prevTab' });
    expect(resolveShortcut(ev('}', 'BracketRight', { meta: true, shift: true }), mac)).toEqual({ id: 'nextTab' });
  });

  it('selects tabs by digit, including on AZERTY', () => {
    expect(resolveShortcut(ev('3', 'Digit3', { ctrl: true }), linux)).toEqual({ id: 'tabIndex', index: 2 });
    expect(resolveShortcut(ev('&', 'Digit1', { ctrl: true }), linux)).toEqual({ id: 'tabIndex', index: 0 });
    expect(resolveShortcut(ev('9', 'Digit9', { meta: true }), mac)).toEqual({ id: 'tabIndex', index: 8 });
    expect(resolveShortcut(ev('0', 'Digit0', { meta: true }), mac)).toEqual({ id: 'fontReset' });
  });

  it('never takes Ctrl+D on Linux and Windows, and splits with Cmd+D on macOS', () => {
    expect(resolveShortcut(ev('d', 'KeyD', { ctrl: true }), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('d', 'KeyD', { ctrl: true }), linux)).toBeNull();
    expect(resolveShortcut(ev('D', 'KeyD', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'splitDown' });
    expect(resolveShortcut(ev('E', 'KeyE', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'splitRight' });
    expect(resolveShortcut(ev('d', 'KeyD', { meta: true }), { ...mac, inTerminal: true })).toEqual({ id: 'splitRight' });
    expect(resolveShortcut(ev('D', 'KeyD', { meta: true, shift: true }), mac)).toEqual({ id: 'splitDown' });
  });

  it('keeps Ctrl+F, Ctrl+B and Ctrl+J for the terminal and offers Ctrl+Shift instead', () => {
    for (const [k, id] of [
      ['f', 'find'],
      ['b', 'toggleSidebar'],
      ['j', 'togglePanel'],
    ] as const) {
      const code = `Key${k.toUpperCase()}`;
      expect(resolveShortcut(ev(k, code, { ctrl: true }), linuxTerm)).toBeNull();
      expect(resolveShortcut(ev(k, code, { ctrl: true }), linux)).toEqual({ id });
      expect(resolveShortcut(ev(k.toUpperCase(), code, { ctrl: true, shift: true }), linuxTerm)).toEqual({ id });
      expect(resolveShortcut(ev(k, code, { meta: true }), { ...mac, inTerminal: true })).toEqual({ id });
    }
    expect(resolveShortcut(ev('/', 'Slash', { ctrl: true }), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('?', 'Slash', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'shortcuts' });
    expect(resolveShortcut(ev('?', 'Slash', { meta: true, shift: true }), mac)).toEqual({ id: 'shortcuts' });
  });

  it('opens source control with Ctrl+Shift+G, even from a terminal, like VS Code', () => {
    expect(resolveShortcut(ev('G', 'KeyG', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'sourceControl' });
    expect(resolveShortcut(ev('G', 'KeyG', { ctrl: true, shift: true }), linux)).toEqual({ id: 'sourceControl' });
    expect(resolveShortcut(ev('G', 'KeyG', { meta: true, shift: true }), mac)).toEqual({ id: 'sourceControl' });
    expect(resolveShortcut(ev('g', 'KeyG', { ctrl: true }), linuxTerm)).toBeNull();
  });

  it('copies and pastes with Ctrl+Shift+C/V only in a terminal and leaves Ctrl+V and macOS native', () => {
    expect(resolveShortcut(ev('C', 'KeyC', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'copy' });
    expect(resolveShortcut(ev('C', 'KeyC', { ctrl: true, shift: true }), linux)).toBeNull();
    expect(resolveShortcut(ev('c', 'KeyC', { ctrl: true }), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('c', 'KeyC', { ctrl: true }), linux)).toBeNull();
    expect(resolveShortcut(ev('c', 'KeyC', { meta: true }), mac)).toBeNull();
    expect(resolveShortcut(ev('V', 'KeyV', { ctrl: true, shift: true }), linuxTerm)).toEqual({ id: 'paste' });
    expect(resolveShortcut(ev('V', 'KeyV', { ctrl: true, shift: true }), linux)).toBeNull();
    expect(resolveShortcut(ev('v', 'KeyV', { ctrl: true }), linuxTerm)).toBeNull();
    expect(resolveShortcut(ev('v', 'KeyV', { meta: true }), { ...mac, inTerminal: true })).toBeNull();
    expect(resolveShortcut(ev('c', 'KeyC', { meta: true }), { ...mac, inTerminal: true })).toBeNull();
    expect(resolveShortcut(ev('W', 'KeyW', { meta: true, shift: true }), mac)).toBeNull();
  });

  it('moves focus between panes with Mod+Alt+arrows', () => {
    expect(resolveShortcut(ev('ArrowLeft', 'ArrowLeft', { meta: true, alt: true }), mac)).toEqual({ id: 'focusLeft' });
    expect(resolveShortcut(ev('ArrowDown', 'ArrowDown', { ctrl: true, alt: true }), linuxTerm)).toEqual({ id: 'focusDown' });
    expect(resolveShortcut(ev('ArrowDown', 'ArrowDown', { ctrl: true }), linuxTerm)).toBeNull();
  });

  it('changes the font size with =, + and -, on QWERTY and AZERTY', () => {
    expect(resolveShortcut(ev('=', 'Equal', { meta: true }), mac)).toEqual({ id: 'fontUp' });
    expect(resolveShortcut(ev('+', 'Equal', { ctrl: true, shift: true }), linux)).toEqual({ id: 'fontUp' });
    expect(resolveShortcut(ev('-', 'Minus', { ctrl: true }), linuxTerm)).toEqual({ id: 'fontDown' });
    expect(resolveShortcut(ev('-', 'Digit6', { ctrl: true }), linux)).toEqual({ id: 'fontDown' });
    expect(resolveShortcut(ev(')', 'Minus', { ctrl: true }), linux)).toBeNull();
    expect(resolveShortcut(ev('+', 'NumpadAdd', { ctrl: true }), linux)).toEqual({ id: 'fontUp' });
  });

  it('reads letters from the layout, not the physical key', () => {
    expect(eventKey({ key: 'w', code: 'KeyZ' })).toBe('W');
    expect(eventKey({ key: 'ц', code: 'KeyW' })).toBe('W');
    expect(resolveShortcut(ev('w', 'KeyZ', { ctrl: true }), linux)).toEqual({ id: 'closeTab' });
  });
});

describe('shortcut labels', () => {
  it('formats per platform', () => {
    expect(formatCombo({ key: 'T', mod: true, shift: true }, true)).toBe('⇧⌘T');
    expect(formatCombo({ key: 'T', mod: true, shift: true }, false)).toBe('Ctrl+Shift+T');
    expect(formatCombo({ key: 'Tab', ctrl: true }, true)).toBe('⌃⇥');
    expect(formatCombo({ key: 'ArrowLeft', mod: true, alt: true }, false)).toBe('Ctrl+Alt+Left');
    expect(shortcutLabel('splitRight', { mac: false, ctrlWClosesTab: true })).toBe('Ctrl+Shift+E');
    expect(shortcutLabel('splitRight', { mac: true, ctrlWClosesTab: true })).toBe('⌘D');
    expect(shortcutLabel('closeTab', { mac: false, ctrlWClosesTab: false })).toBe('');
  });

  it('builds the sheet without unbound rows', () => {
    const linuxRows = sheetRows({ mac: false, ctrlWClosesTab: false }).flatMap((s) => s.rows.map((r) => r.id));
    expect(linuxRows).not.toContain('closeTab');
    expect(linuxRows).not.toContain('closeWindow');
    const macRows = sheetRows({ mac: true, ctrlWClosesTab: false }).flatMap((s) => s.rows);
    expect(macRows.find((r) => r.id === 'closeWindow')?.keys[0].label).toBe('⇧⌘W');
    const find = sheetRows({ mac: false, ctrlWClosesTab: true })
      .flatMap((s) => s.rows)
      .find((r) => r.id === 'find');
    expect(find?.keys).toEqual([
      { label: 'Ctrl+Shift+F', outsideTerminal: false },
      { label: 'Ctrl+F', outsideTerminal: true },
    ]);
  });
});
