import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(lastAssistantText: string) {
  vi.resetModules();
  const activity = { l1: { workLog: [], todos: [], editedFiles: [], needsInput: false, lastAssistantText } };
  const { app } = await import('../../state.svelte');
  app.data.activity = activity as never;
  app.data.lines = [{ line: { id: 'l1', agent: 'claude', state: 'active' }, project: 'p' }] as never;
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

describe('SidePanel last message', () => {
  it('shows the last message without markdown code backticks', async () => {
    const html = await rendered('Run `pnpm test` then ``pnpm lint``');
    expect(html).toContain('Run pnpm test then pnpm lint');
    expect(html).not.toContain('`');
  });
});
