import { describe, expect, it } from 'vitest';
import { tabLabel } from './tab-label';
import type { TabInfo, TabKind } from './tabs';

const tab = (kind: TabKind, arg = '', key = `${kind}:${arg}`): TabInfo => ({ key, kind, arg });

describe('tabLabel', () => {
  it('uses the session title for a session tab', () => {
    const item = { title: 'My session' } as TabInfo['item'];
    expect(tabLabel({ ...tab('agent'), item }, { browserHost: true })).toBe('My session');
    expect(tabLabel({ ...tab('agent'), item }, { browserHost: false })).toBe('My session');
  });

  it('shows the host of a browser tab on the desktop tab bar', () => {
    expect(tabLabel(tab('browser', 'https://example.com/a/b'), { browserHost: true })).toBe('example.com');
  });

  it('shows the whole text when a browser address has no valid URL', () => {
    expect(tabLabel(tab('browser', 'not a url'), { browserHost: true })).toBe('not a url');
  });

  it('falls back to the plain browser name without an address', () => {
    expect(tabLabel(tab('browser', ''), { browserHost: true })).toBe('Browser');
  });

  it('keeps the plain browser name on the phone chips', () => {
    expect(tabLabel(tab('browser', 'https://example.com'), { browserHost: false })).toBe('Browser');
  });

  it('names the panel tabs', () => {
    expect(tabLabel(tab('system', 'host'), { browserHost: true })).toBe('System');
    expect(tabLabel(tab('usage', 'host'), { browserHost: true })).toBe('Usage');
  });

  it('falls back to the key for an unknown kind', () => {
    expect(tabLabel(tab('shell', '', 'weird:1'), { browserHost: true })).toBe('weird:1');
  });
});
