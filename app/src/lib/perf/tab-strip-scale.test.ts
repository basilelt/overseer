import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { settings } from '../settings/settings.svelte';
import { app } from '../state.svelte';
import { windowSpan } from '../tabs/tab-window';
import type { LocalLine } from '../types';
import TabBar from '../components/workspace/TabBar.svelte';

const before = { data: app.data, wrap: settings.wrapTabs };
afterEach(() => {
  app.data = before.data;
  settings.wrapTabs = before.wrap;
});

const SPAN = windowSpan(1440);

function strip(n: number, active: number) {
  const lines: LocalLine[] = Array.from({ length: n }, (_, i) => ({
    line: { id: `l${i}`, agent: 'claude', state: 'held', lastActivityAt: '2026-11-05T10:00:00Z' },
    local: { title: `Session ${i}` },
  }));
  app.data = { ...app.data, lines, activity: {} };
  const tabs = lines.map((l) => `line:${l.line.id}`);
  const html = render(TabBar, { props: { pane: { tabs, active: tabs[active] ?? null }, index: 0, focused: true, canClose: false } }).body;
  const keys = [...html.matchAll(/data-tab="line:l(\d+)"/g)].map((m) => Number(m[1]));
  return { html, keys };
}

describe('tab strip: mounted tabs do not grow with the tabs of the pane', () => {
  it('mounts the same number of tabs for 50 and for 500', () => {
    const small = strip(50, 10).keys.length;
    const big = strip(500, 10).keys.length;
    expect(small).toBe(SPAN);
    expect(big).toBe(small);
  });

  it('mounts every tab of a pane that fits the window, as before', () => {
    expect(strip(SPAN, 3).keys).toEqual(Array.from({ length: SPAN }, (_, i) => i));
    expect(strip(SPAN, 3).html).not.toContain('class="gap');
  });

  it('mounts the tabs around an active tab far down the list, with spacers for the rest', () => {
    const { keys, html } = strip(500, 400);
    expect(keys).toContain(400);
    expect(keys.length).toBe(SPAN);
    expect(keys[0]).toBeGreaterThan(400 - SPAN);
    expect(keys.at(-1)).toBeLessThan(400 + SPAN);
    expect(html.match(/class="gap[^"]*"/g)?.length).toBe(2);
    expect(html).toMatch(/aria-selected="true"/);
  });

  it('numbers the tabs by their place in the pane, not in the window', () => {
    const { html } = strip(500, 400);
    expect(html).not.toMatch(/<span class="num[^"]*">[1-9]<\/span>/);
    expect(strip(500, 0).html).toMatch(/<span class="num[^"]*">1<\/span>/);
  });

  it('mounts every tab when tabs wrap on several rows', () => {
    settings.wrapTabs = true;
    expect(strip(200, 0).keys.length).toBe(200);
  });
});
