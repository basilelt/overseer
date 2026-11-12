import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { t } from '../../i18n.svelte';
import { app } from '../../state.svelte';
import type { LocalLine } from '../../types';
import StatesView from './StatesView.svelte';

const saved = app.data;
afterEach(() => {
  app.data = saved;
});

const line = (id: string, title: string, local: LocalLine['local'] = {}): LocalLine => ({ line: { id, agent: 'claude', state: 'active', lastActivityAt: new Date().toISOString() }, local: { title, ...local } });
const html = (filter = '', lines: LocalLine[] = []) => {
  app.data = { ...app.data, lines };
  return render(StatesView, { props: { filter } }).body;
};

describe('StatesView', () => {
  it('tells a filter without match from an empty list', () => {
    expect(html()).toContain(t('sidebar.empty'));
    const filtered = html('zzz', [line('a', 'Fix login', { pid: 1, busy: true })]);
    expect(filtered).toContain(t('common.none'));
    expect(filtered).not.toContain(t('sidebar.empty'));
  });

  it('lists live sessions under their state section, skipping the empty sections', () => {
    const h = html('', [line('a', 'Fix login', { pid: 1, busy: true })]);
    expect(h).toContain('Fix login');
    expect(h.match(/<section class="section/g)).toHaveLength(1);
    expect(h).toContain(t('ws.section.running'));
    expect(h).not.toContain(t('sidebar.empty'));
  });

  it('collapses the closed sessions into time buckets, folded by default', () => {
    const h = html('', [line('b', 'Old work')]);
    expect(h).toContain(t('ws.section.closed'));
    expect(h).toMatch(/<button class="bucket[^"]*" type="button" aria-expanded="false"/);
    expect(h).not.toContain('Old work');
  });
});
