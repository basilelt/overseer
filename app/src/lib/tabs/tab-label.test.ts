import { describe, expect, it } from 'vitest';
import { tabLabel } from './tab-label';
import type { TabInfo, TabKind } from './tabs';

const tab = (kind: TabKind, arg = '', key = `${kind}:${arg}`): TabInfo => ({ key, kind, arg });

describe('tabLabel', () => {
  it('uses the session title for a session tab', () => {
    const item = { title: 'My session' } as TabInfo['item'];
    expect(tabLabel({ ...tab('agent'), item })).toBe('My session');
  });

  it('shows the host of a browser tab on the tab bar and on the phone chips', () => {
    expect(tabLabel(tab('browser', 'https://example.com/a/b'))).toBe('example.com');
  });

  it('shows the whole text when a browser address has no valid URL', () => {
    expect(tabLabel(tab('browser', 'not a url'))).toBe('not a url');
  });

  it('falls back to the plain browser name without an address', () => {
    expect(tabLabel(tab('browser', ''))).toBe('Browser');
  });

  it('names the panel tabs', () => {
    expect(tabLabel(tab('system', 'host'))).toBe('System');
    expect(tabLabel(tab('usage', 'host'))).toBe('Usage');
  });

  it('names graph, worktree and review tabs after their project or target', () => {
    expect(tabLabel(tab('graph', '/work/repo'))).toContain('repo');
    expect(tabLabel(tab('worktrees', '/work/repo'))).toContain('repo');
    expect(tabLabel(tab('review', 'main'))).toContain('main');
  });

  it('names the pull requests tab', () => {
    expect(tabLabel(tab('prs', 'host'))).not.toBe('prs:host');
  });

  it('uses the whole address when the host is empty', () => {
    expect(tabLabel(tab('browser', 'file:///tmp/a.html'))).toBe('file:///tmp/a.html');
  });

  it('falls back to the key for an unknown kind', () => {
    expect(tabLabel(tab('shell', '', 'weird:1'))).toBe('weird:1');
  });
});
