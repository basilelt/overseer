import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(lastAssistantText: string, extra: { line?: object; activity?: object; git?: boolean; selected?: object | null } = {}) {
  vi.resetModules();
  const activity = { l1: { workLog: [], todos: [], editedFiles: [], needsInput: false, lastAssistantText, ...extra.activity } };
  const { app } = await import('../../state.svelte');
  app.data.activity = activity as never;
  app.data.lines = [{ line: { id: 'l1', agent: 'claude', state: 'active', ...extra.line }, project: 'p' }] as never;
  app.selectedKey = 'line:l1';
  app.panelGit = extra.git ?? false;
  vi.spyOn(app, 'selected', 'get').mockReturnValue((extra.selected === undefined ? { key: 'line:l1', lineId: 'l1', title: 'x' } : extra.selected) as never);
  const { render } = await import('svelte/server');
  const { default: SidePanel } = await import('./SidePanel.svelte');
  let html = render(SidePanel, { props: {} }).body;
  for (let prev = ''; prev !== html; ) {
    prev = html;
    html = html.replace(/<!--[\s\S]*?-->/g, '');
  }
  return html;
}

afterEach(() => vi.restoreAllMocks());

describe('SidePanel last message', () => {
  it('shows the last message without markdown code backticks', async () => {
    const html = await rendered('Run `pnpm test` then ``pnpm lint``');
    expect(html).toContain('Run pnpm test then pnpm lint');
    expect(html).not.toContain('`');
  });

  it('keeps pin in the panel and leaves pause, resume and close to the chat header', async () => {
    const html = await rendered('hello');
    expect(html).toContain('Pin');
    for (const label of ['Hold', 'Resume', 'Close line']) expect(html).not.toContain(label);
  });
});

describe('SidePanel outside a git repository', () => {
  it('shows the activity sections of a session whose folder is not a git repository', async () => {
    const html = await rendered('done', {
      line: { repo: 'unknown', cwd: '/home/u' },
      activity: {
        workLog: [{ kind: 'read', label: 'Read a.ts', state: 'done', at: '2026-01-01T00:00:00Z' }],
        todos: [{ text: 'step', state: 'completed' }],
        editedFiles: ['/home/u/a.ts'],
      },
    });
    expect(html).toContain('Last message');
    expect(html).toContain('Work log');
    expect(html).toContain('Todos');
    expect(html).toContain('Edited files');
    expect(html).toContain('Not a git repository');
    expect(html).not.toContain('No activity yet');
  });
});

describe('SidePanel cost', () => {
  const cost = {
    input: 10, cacheWrite: 2000, cacheRead: 500_000, output: 3000, cost: 1.234, partial: false,
    model: 'claude-opus-5-5', effort: 'high', pricesAsOf: '2026-11-11',
    models: [{ model: 'claude-opus-5-5', input: 10, cacheWrite: 2000, cacheRead: 500_000, output: 3000, cost: 1.234 }],
    switches: [],
  };

  it('shows the session cost with its model and effort, and a cost section', async () => {
    const html = await rendered('hello', { activity: { cost } });
    expect(html).toContain('≈$1.23 · claude-opus-5-5 · effort high');
    expect(html).toContain('data-sec="cost"');
    expect(html).toContain('cache read 500k');
  });

  it('shows no cost when the transcript has no token counts', async () => {
    const html = await rendered('hello');
    expect(html).not.toContain('data-sec="cost"');
    expect(html).not.toContain('≈$');
  });
});

describe('SidePanel tabs', () => {
  const tab = (html: string, label: string) => html.match(new RegExp(`<button[^>]*role="tab"[^>]*aria-selected="(true|false)"[^>]*>(?:<svg[\\s\\S]*?</svg>)?${label}</button>`))?.[1];

  it('opens on the Activity tab with the Git tab next to it', async () => {
    const html = await rendered('hello');
    expect(tab(html, 'Activity')).toBe('true');
    expect(tab(html, 'Git')).toBe('false');
    expect(html).toContain('Last message');
  });

  it('swaps the session activity for source control on the Git tab', async () => {
    const html = await rendered('hello', { git: true });
    expect(tab(html, 'Activity')).toBe('false');
    expect(tab(html, 'Git')).toBe('true');
    expect(html).not.toContain('Last message');
    expect(html).not.toContain('Open review');
  });

  it('keeps both tabs when no session is selected', async () => {
    const html = await rendered('hello', { selected: null });
    expect(html).toContain('No session selected');
    expect(tab(html, 'Git')).toBe('false');
  });
});
