import { afterEach, describe, expect, it, vi } from 'vitest';
import { toHex6 } from './color-hex';

afterEach(() => vi.unstubAllGlobals());

describe('toHex6', () => {
  it('keeps a six digit hex, lowercased', () => {
    expect(toHex6('#1A2b3C')).toBe('#1a2b3c');
  });

  it('expands the three digit shorthand a color input would reject', () => {
    expect(toHex6('#fff')).toBe('#ffffff');
    expect(toHex6(' #0aF ')).toBe('#00aaff');
  });

  it('converts rgb and rgba', () => {
    expect(toHex6('rgb(255, 0, 16)')).toBe('#ff0010');
    expect(toHex6('rgba(1 2 3 / 50%)')).toBe('#010203');
    expect(toHex6('rgb(300, 0, 0)')).toBe('#ff0000');
  });

  it('resolves a named color through the browser', () => {
    const probe = { style: { color: '' }, remove: vi.fn() };
    vi.stubGlobal('document', { createElement: () => probe, body: { append: vi.fn() } });
    vi.stubGlobal('getComputedStyle', () => ({ color: 'rgb(255, 165, 0)' }));
    expect(toHex6('orange')).toBe('#ffa500');
    expect(probe.style.color).toBe('orange');
    expect(probe.remove).toHaveBeenCalled();
  });

  it('falls back to black for an unusable value', () => {
    expect(toHex6('')).toBe('#000000');
    expect(toHex6('var(--x)')).toBe('#000000');
    expect(toHex6('notacolor')).toBe('#000000');
  });
});
