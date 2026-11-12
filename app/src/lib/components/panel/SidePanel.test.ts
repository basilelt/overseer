import { afterEach, describe, expect, it, vi } from 'vitest';
import source from './SidePanel.svelte?raw';

async function rendered(lastAssistantText: string, extra: { line?: object; activity?: object } = {}) {
  vi.resetModules();
  const activity = { l1: { workLog: [], todos: [], editedFiles: [], needsInput: false, lastAssistantText, ...extra.activity } };
  const { app } = await import('../../state.svelte');
  app.data.activity = activity as never;
  app.data.lines = [{ line: { id: 'l1', agent: 'claude', state: 'active', ...extra.line }, project: 'p' }] as never;
  app.selectedKey = 'line:l1';
  vi.spyOn(app, 'selected', 'get').mockReturnValue({ key: 'line:l1', lineId: 'l1', title: 'x' } as never);
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

describe('SidePanel section drag', () => {
  it('clears temporary drag state when the pointer is canceled or the panel unmounts', () => {
    expect(source).toMatch(/window\.addEventListener\('pointercancel', dragCancel\)/);
    expect(source).toMatch(/window\.addEventListener\('blur', dragCancel\)/);
    expect(source).toMatch(/function dragCancel\(\) \{\s*cancelDrag\(\);\s*\}/);
    expect(source).toMatch(/return \(\) => \{[\s\S]*cancelDrag\(\)/);
  });
});

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
  it('loads activity for every selected line and keeps the repo guard only for diff and related', () => {
    const effect = source.slice(source.indexOf('$effect(() => {'), source.indexOf('onMount('));
    expect(effect).toMatch(/if \(!id \|\| !app\.client\) \{/);
    expect(effect).toMatch(/loadPanel\(id, inGit, \{ activity: \(x\) => app\.loadActivity\(x\), diff: loadDiff, related: loadRelated \}\)/);
    expect(source).toMatch(/async function loadDiff\(id: string\) \{\s*if \(!repo\) return;/);
    expect(source).toMatch(/async function loadRelated\(id: string\) \{\s*if \(!app\.client \|\| !repo\) return;/);
  });

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
