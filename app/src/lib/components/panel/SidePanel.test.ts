import { afterEach, describe, expect, it, vi } from 'vitest';
import source from './SidePanel.svelte?raw';

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
    // Given a selected session
    // When the panel renders
    // Then it offers Pin but none of the session-state actions
    const html = await rendered('hello');
    expect(html).toContain('Pin');
    for (const label of ['Hold', 'Resume', 'Close line']) expect(html).not.toContain(label);
  });
});
