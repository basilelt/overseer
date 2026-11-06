import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { SidebarItem } from '../../grouping';
import SessionCard from './SessionCard.svelte';

const html = (over: Partial<SidebarItem>) =>
  render(SessionCard, {
    props: { item: { key: 'line:a', kind: 'claude', group: 'running', state: 'running', title: 'A', at: '2026-11-05T10:00:00Z', ...over }, selected: false, now: Date.parse('2026-11-05T10:05:00Z'), onselect: () => {} },
  }).body;

describe('SessionCard', () => {
  it('shows the process state next to the title, whatever the group', () => {
    // Given a session sitting in the needs-testing group while its process is paused
    // When the card renders
    // Then the row reads Paused, not the group
    expect(html({ group: 'needs_testing', state: 'paused' })).toContain('>Paused<');
    expect(html({ state: 'running' })).toContain('>Running<');
  });
});
