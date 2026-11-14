import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { declOf, parseStyle } from '../../../test/css';
import { HostLink } from '../../host/host-link.svelte';
import { MockDaemon } from '../../daemon/mock';
import { settings } from '../../settings/settings.svelte';
import { app } from '../../state.svelte';
import { newSessionItems } from '../../workspace/wsactions';
import type { AgentInfo, LocalLine, Task } from '../../types';
import TabBar from './TabBar.svelte';
import bar from './TabBar.svelte?raw';

const before = { data: app.data, link: app.link, others: app.others, agents: app.agents, defaultAgent: settings.defaultAgent };
afterEach(() => {
  Object.assign(app, { data: before.data, link: before.link, others: before.others, agents: before.agents });
  settings.defaultAgent = before.defaultAgent;
  vi.restoreAllMocks();
});

const cost = { input: 1, cacheWrite: 0, cacheRead: 0, output: 1, cost: 1.234, partial: false, model: 'm', effort: 'high', pricesAsOf: '2026-11-11', models: [], switches: [] };
const line = (): LocalLine => ({ line: { id: 'l1', agent: 'claude', state: 'active', lastActivityAt: '2026-11-05T10:00:00Z' }, local: { title: 'Fix login' } });
const html = (activity: Record<string, unknown> = {}) => {
  app.data = { ...app.data, lines: [line()], activity: activity as never };
  return render(TabBar, { props: { pane: { tabs: ['line:l1'], active: 'line:l1' }, index: 0, focused: true, canClose: true } }).body;
};

describe('tab strip content', () => {
  it('keeps the session cost off its tab, the side panel shows it', () => {
    const h = html({ l1: { workLog: [], todos: [], editedFiles: [], needsInput: false, lastAssistantText: '', cost } });
    expect(h).not.toContain('$1.23');
    expect(h).not.toMatch(/class="[^"]*\bcost\b/);
  });

  it('colours the dot of a running session in testing by its state, like the sidebar row', () => {
    const busy: LocalLine = { ...line(), local: { title: 'Fix login', pid: 1, busy: true } };
    app.data = { ...app.data, lines: [busy], tasks: [{ id: 't1', lineId: 'l1', status: 'in_testing', title: 'T', description: '' } as Task], activity: {} };
    const h = render(TabBar, { props: { pane: { tabs: ['line:l1'], active: 'line:l1' }, index: 0, focused: true, canClose: true } }).body;
    expect(h).toMatch(/class="dot[^"]*" style="background: var\(--state-running\);"/);
    expect(h).not.toContain('--st-testing');
  });

  it('puts the tab number and the close button in one slot, so hovering does not change the width', () => {
    const h = html();
    expect(h).toMatch(/<span class="slot[^"]*"><span class="num[^"]*">1<\/span><button class="x[^"]*"/);
  });

  it('does not name the host on tabs, even with several hosts in the window, the sidebar already does', () => {
    const local = new HostLink({ kind: 'local', label: 'Local' }, new MockDaemon({ live: false }));
    const prod = new HostLink({ kind: 'ssh', host: 'prod', label: 'prod' }, new MockDaemon({ live: false }));
    Object.assign(app, { link: local, others: [prod] });
    const h = html();
    expect(h).not.toContain('class="host');
    expect(h).not.toContain('>Local<');
  });
});

describe('new tab menu', () => {
  const agent = (id: string, over: Partial<AgentInfo> = {}): AgentInfo => ({ id, name: id.toUpperCase(), installed: true, efforts: [], resume: false, verified: true, sessions: true, turns: true, ...over });

  it('only starts sessions: agents, the checkout switch and a shell, no view', () => {
    app.agents = [agent('claude'), agent('codex'), agent('shell')];
    const items = newSessionItems().filter((i) => !i.separator);
    expect(items.map((i) => i.agent)).toEqual(['claude', 'codex', undefined, 'shell']);
    expect(items[2].checked).toBe(settings.sessionsInCheckout);
  });

  it('marks the default agent', () => {
    app.agents = [agent('claude'), agent('codex')];
    settings.defaultAgent = 'codex';
    const items = newSessionItems();
    expect(items.find((i) => i.agent === 'codex')?.default).toBe(true);
    expect(items.find((i) => i.agent === 'claude')?.default).toBe(false);
  });

  it('installs an absent agent from its entry instead of greying it out, and greys out one with no install command', () => {
    app.agents = [agent('claude'), agent('aider', { installed: false, installable: true, install: 'pip install aider' }), agent('goose', { installed: false })];
    const group = newSessionItems().find((i) => i.more)!;
    const [aider, goose] = group.more!;
    expect(aider.disabled).toBe(false);
    expect(aider.hint).toBe('pip install aider');
    expect(goose.disabled).toBe(true);
    const install = vi.spyOn(app, 'installAgent').mockResolvedValue(undefined as never);
    aider.run!();
    expect(install).toHaveBeenCalledWith('aider');
  });
});

describe('CSS guard: tab strip', () => {
  it('keeps tabs at their own width so the strip scrolls instead of squeezing them', () => {
    expect(declOf(bar, '.tab', 'flex')).toBe('none');
    expect(declOf(bar, '.tabs', 'overflow-x')).toBe('auto');
  });

  it('caps an inactive tab and lets the active one show its full title', () => {
    expect(declOf(bar, '.tab', 'max-width')).toMatch(/^\d+px$/);
    expect(declOf(bar, '.tab.active', 'max-width')).toBe('none');
  });

  it('hides the scrollbar, which WebKitGTK draws over the titles', () => {
    expect(declOf(bar, '.tabs', 'scrollbar-width')).toBe('none');
    expect(declOf(bar, '.tabs::-webkit-scrollbar', 'display')).toBe('none');
    expect(parseStyle(bar).some((r) => r.selector === '.tabs:hover')).toBe(false);
  });

  it('stacks the number and the close button in one grid cell, hiding the number on hover', () => {
    expect(declOf(bar, '.slot > *', 'grid-area')).toBe('1 / 1');
    expect(declOf(bar, '.tab:hover .num', 'visibility')).toBe('hidden');
    expect(declOf(bar, '.x', 'z-index')).toBeUndefined();
  });

  it('moves the pane toolbar under the tabs in a narrow pane instead of squeezing the strip', () => {
    expect(declOf(bar, '.tabbar-box', 'container-type')).toBe('inline-size');
    expect(declOf(bar, '.actions', 'flex')).toBe('none');
    const narrow = parseStyle(bar).filter((r) => r.at.startsWith('@container'));
    expect(narrow.find((r) => r.selector === '.tabs')?.decls.flex).toBe('1 0 100%');
    expect(narrow.find((r) => r.selector === '.actions')?.decls['border-left']).toBe('0');
  });

  it('keeps the toolbar off the tab strip with a separator', () => {
    expect(declOf(bar, '.actions', 'margin-left')).toBe('6px');
    expect(declOf(bar, '.actions', 'border-left')).toBe('1px solid var(--border)');
  });

  it('wraps the toolbar buttons in a very narrow pane so none is clipped', () => {
    const actions = parseStyle(bar).filter((r) => r.at.startsWith('@container') && r.selector === '.actions');
    expect(actions.some((r) => r.decls['flex-wrap'] === 'wrap' && r.decls['min-height'] === '34px')).toBe(true);
  });
});
