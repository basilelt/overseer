import { afterEach, describe, expect, it, vi } from 'vitest';
import type { LocalLine } from '../../types';

async function rendered(local: LocalLine['local'], termId?: string) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  app.data = { lines: [{ line: { id: 'l1', agent: 'claude', state: 'held', lastActivityAt: '' }, local }], terms: [], tasks: [], activity: {} };
  const { render } = await import('svelte/server');
  const { default: ChatView } = await import('./ChatView.svelte');
  const item = { key: 'line:l1', kind: 'claude', group: 'stopped', state: 'closed', title: 'x', at: '', lineId: 'l1', termId };
  return render(ChatView, { props: { item: item as never, phone: true } }).body;
}

afterEach(() => vi.restoreAllMocks());

describe('ChatView while a resumed agent starts', () => {
  it('points to the terminal, where the agent may be asking something before it starts', async () => {
    // Given a stopped session that was resumed into a live terminal, before the agent reports its session
    // When the chat renders
    // Then it reads Starting and offers to open the terminal
    const h = await rendered({ heldReason: 'lost' }, 't1');
    expect(h).toContain('Starting');
    expect(h).toContain('answer it in the terminal');
    expect(h).toContain('Open terminal');
  });

  it('shows no such note on a stopped session with no terminal', async () => {
    const h = await rendered({ heldReason: 'lost' });
    expect(h).toContain('Closed');
    expect(h).not.toContain('answer it in the terminal');
  });
});
