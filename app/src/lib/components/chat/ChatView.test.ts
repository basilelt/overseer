import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { Component } from 'svelte';
import type { LocalLine } from '../../types';
import { app } from '../../state.svelte';

let ChatView: Component<{ item: never; phone: boolean }>;
const initial = app.data;

beforeAll(async () => {
  ChatView = (await import('./ChatView.svelte')).default as never;
}, 60_000);

afterEach(() => {
  app.data = initial;
});

function rendered(local: LocalLine['local'], termId?: string) {
  app.data = { lines: [{ line: { id: 'l1', agent: 'claude', state: 'held', lastActivityAt: '' }, local }], terms: [], tasks: [], activity: {} };
  const item = { key: 'line:l1', kind: 'claude', group: 'stopped', state: 'closed', title: 'x', at: '', lineId: 'l1', termId };
  return render(ChatView, { props: { item: item as never, phone: true } }).body;
}

describe('ChatView while a resumed agent starts', () => {
  it('points to the terminal, where the agent may be asking something before it starts', () => {
    const h = rendered({ heldReason: 'lost' }, 't1');
    expect(h).toContain('Starting');
    expect(h).toContain('answer it in the terminal');
    expect(h).toContain('Open terminal');
  });

  it('shows no such note on a stopped session with no terminal', () => {
    const h = rendered({ heldReason: 'lost' });
    expect(h).toContain('Closed');
    expect(h).not.toContain('answer it in the terminal');
  });
});
