import { describe, expect, it } from 'vitest';
import { applyTextDefaults, isTextField, startTextDefaults } from './textinput';

function el(tagName: string, attrs: Record<string, string> = {}) {
  const a = new Map(Object.entries(attrs));
  return {
    tagName,
    attrs: a,
    hasAttribute: (n: string) => a.has(n),
    getAttribute: (n: string) => a.get(n) ?? null,
    setAttribute: (n: string, v: string) => void a.set(n, v),
  };
}

describe('text field defaults', () => {
  it('turns off writing tools, autocorrect, capitalisation and spellcheck on plain fields', () => {
    const i = el('INPUT');
    applyTextDefaults(i);
    expect(Object.fromEntries(i.attrs)).toEqual({ writingsuggestions: 'false', autocorrect: 'off', autocapitalize: 'off', spellcheck: 'false' });
  });

  it('keeps spellcheck on prose boxes and never overrides explicit attributes', () => {
    const t = el('TEXTAREA', { spellcheck: 'true' });
    applyTextDefaults(t);
    expect(Object.fromEntries(t.attrs)).toEqual({ spellcheck: 'true', writingsuggestions: 'false' });
    const s = el('INPUT', { autocapitalize: 'words' });
    applyTextDefaults(s);
    expect(s.attrs.get('autocapitalize')).toBe('words');
  });

  it('ignores non-text controls', () => {
    for (const type of ['checkbox', 'range', 'file', 'color']) expect(isTextField(el('INPUT', { type }))).toBe(false);
    const b = el('BUTTON');
    applyTextDefaults(b);
    expect(b.attrs.size).toBe(0);
    expect(isTextField(el('INPUT', { type: 'Search' }))).toBe(true);
  });

  it('applies on focus through one capturing listener', () => {
    const calls: [string, unknown, unknown][] = [];
    let handler: ((e: Event) => void) | null = null;
    const doc = {
      addEventListener: (n: string, fn: (e: Event) => void, c: unknown) => {
        calls.push(['add', n, c]);
        handler = fn;
      },
      removeEventListener: (n: string, _fn: unknown, c: unknown) => void calls.push(['remove', n, c]),
    };
    const stop = startTextDefaults(doc as unknown as Document);
    const t = el('TEXTAREA');
    handler!({ target: t } as unknown as Event);
    expect(t.attrs.get('writingsuggestions')).toBe('false');
    stop();
    expect(calls).toEqual([
      ['add', 'focusin', true],
      ['remove', 'focusin', true],
    ]);
  });
});
