import { describe, expect, it } from 'vitest';
import { isEditable, nativeMenu } from './native-menu';

const desktop = { tauri: true, phone: false };

function el(tagName: string, extra: { type?: string; isContentEditable?: boolean } = {}): Element {
  return { tagName, ...extra } as unknown as Element;
}

describe('nativeMenu', () => {
  it('keeps the browser menu outside the Tauri app', () => {
    expect(nativeMenu({ tauri: false, phone: false }, el('P'), '')).toBe('native');
  });

  it('keeps the native Cut, Copy and Paste menu in text fields', () => {
    expect(nativeMenu(desktop, el('INPUT', { type: 'text' }), '')).toBe('native');
    expect(nativeMenu(desktop, el('TEXTAREA'), 'abc')).toBe('native');
    expect(nativeMenu(desktop, el('DIV', { isContentEditable: true }), '')).toBe('native');
  });

  it('drops the menu with Back, Forward and Reload over plain content', () => {
    expect(nativeMenu(desktop, el('P'), '')).toBe('none');
    expect(nativeMenu(desktop, el('INPUT', { type: 'checkbox' }), '')).toBe('none');
    expect(nativeMenu(desktop, null, '')).toBe('none');
  });

  it('offers the app menu with Copy over a selection in a read-only view', () => {
    expect(nativeMenu(desktop, el('PRE'), 'abc')).toBe('copy');
    expect(nativeMenu(desktop, el('PRE'), '  \n')).toBe('none');
  });

  it('leaves the phone alone', () => {
    expect(nativeMenu({ tauri: true, phone: true }, el('P'), '')).toBe('native');
  });

  it('knows editable elements', () => {
    expect(isEditable(null)).toBe(false);
    expect(isEditable(el('INPUT', { type: 'range' }))).toBe(false);
    expect(isEditable(el('INPUT', { type: 'search' }))).toBe(true);
  });
});
