import { describe, expect, it, vi } from 'vitest';
import { linkItems } from './terminal/linkmenu';
import { isWebLink, resolveLink, type LinkPrefs } from './links';

const prefs = (p: Partial<LinkPrefs> = {}): LinkPrefs => ({ linkTarget: 'overseer', linkBrowser: '', linkAsk: false, ...p });
const firefox = [{ id: 'firefox', name: 'Firefox' }];
const desk = { phone: false, browsers: firefox };

describe('link routing', () => {
  it('follows the default target setting', () => {
    expect(resolveLink(prefs(), {}, desk)).toEqual({ kind: 'overseer' });
    expect(resolveLink(prefs({ linkTarget: 'default' }), {}, desk)).toEqual({ kind: 'default' });
    expect(resolveLink(prefs({ linkTarget: 'browser', linkBrowser: 'firefox' }), {}, desk)).toEqual({ kind: 'browser', id: 'firefox' });
  });

  it('asks every time when set, unless a modifier picks the target', () => {
    expect(resolveLink(prefs({ linkAsk: true }), {}, desk)).toEqual({ kind: 'ask' });
    expect(resolveLink(prefs({ linkAsk: true }), { mod: true }, desk)).toEqual({ kind: 'default' });
  });

  it('opens the default browser on Cmd/Ctrl+click and the picker on Shift+click', () => {
    for (const target of ['overseer', 'default', 'browser'] as const) {
      const p = prefs({ linkTarget: target, linkBrowser: 'firefox' });
      expect(resolveLink(p, { mod: true }, desk)).toEqual({ kind: 'default' });
      expect(resolveLink(p, { shift: true }, desk)).toEqual({ kind: 'ask' });
      expect(resolveLink(p, { shift: true, mod: true }, desk)).toEqual({ kind: 'ask' });
    }
  });

  it('treats Cmd/Ctrl in a terminal as the plain link click', () => {
    expect(resolveLink(prefs(), { mod: true }, { ...desk, inTerminal: true })).toEqual({ kind: 'overseer' });
    expect(resolveLink(prefs(), { mod: true, shift: true }, { ...desk, inTerminal: true })).toEqual({ kind: 'ask' });
  });

  it('falls back to the default browser on phones or when the chosen browser is gone', () => {
    const p = prefs({ linkTarget: 'browser', linkBrowser: 'firefox' });
    expect(resolveLink(p, {}, { phone: true, browsers: firefox })).toEqual({ kind: 'default' });
    expect(resolveLink(p, {}, { phone: false, browsers: [] })).toEqual({ kind: 'default' });
    expect(resolveLink(prefs({ linkTarget: 'browser' }), {}, desk)).toEqual({ kind: 'default' });
    expect(resolveLink(p, {}, { phone: false, browsers: null })).toEqual({ kind: 'browser', id: 'firefox' });
    expect(resolveLink(prefs(), {}, { phone: true })).toEqual({ kind: 'overseer' });
  });

  it('lists Overseer, the default browser, each installed browser and Copy link', () => {
    const act = { inside: vi.fn(), outside: vi.fn(), copy: vi.fn() };
    const items = linkItems([{ id: 'firefox', name: 'Firefox' }], act);
    expect(items.map((i) => i.label)).toEqual(['Open in Overseer', 'Open in default browser', 'Open in Firefox', '', 'Copy link']);
    items[1].run?.();
    items[2].run?.();
    expect(act.outside.mock.calls).toEqual([[{ kind: 'default' }], [{ kind: 'browser', id: 'firefox' }]]);
    expect(linkItems([], act).map((i) => i.label)).toEqual(['Open in Overseer', 'Open in default browser', '', 'Copy link']);
  });

  it('only routes web links', () => {
    expect(isWebLink('https://a.dev')).toBe(true);
    expect(isWebLink('HTTP://localhost:3000/x')).toBe(true);
    expect(isWebLink('file:///etc/passwd')).toBe(false);
    expect(isWebLink('javascript:alert(1)')).toBe(false);
  });
});
