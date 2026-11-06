import { describe, expect, it } from 'vitest';
import { GROUP_COLOR, STATE_COLOR } from './AgentIcon.svelte';
import { WS_COLOR } from '../../ws-color';
import chatSrc from '../chat/ChatView.svelte?raw';

// Old group/workspace ids that name a session state, and the state they stand for.
const SAME = { needs_input: 'attention', running: 'running', ready: 'open', paused: 'paused', stopped: 'closed' } as const;

describe('one palette for session states', () => {
  it('colours the tab, phone chip, title bar and group dots like the session row', () => {
    // Given the colour of each state on a session row
    // When a group dot shows the same state
    // Then it uses the same token
    for (const [group, state] of Object.entries(SAME)) expect(GROUP_COLOR[group as keyof typeof SAME]).toBe(STATE_COLOR[state]);
  });

  it('colours the workspace and group dots like the session row', () => {
    // Given the colour of each state on a session row
    // When a workspace aggregates to that state
    // Then its dot uses the same token
    for (const [ws, state] of Object.entries(SAME)) expect(WS_COLOR[ws as keyof typeof SAME]).toBe(STATE_COLOR[state]);
  });

  it('colours the chat header dot with the state tokens only', () => {
    // Given the chat view styles
    const css = chatSrc.slice(chatSrc.indexOf('<style'));
    // When the header dot is drawn for each state
    const dot = (cls: string) => css.match(new RegExp(`\\.state\\.${cls} \\{[^}]*\\}`))?.[0] ?? '';
    // Then it reads the matching --state-* token
    expect(dot('input')).toContain('var(--state-attention)');
    expect(dot('busy')).toContain('var(--state-running)');
    expect(dot('idle')).toContain('var(--state-open)');
    expect(dot('paused')).toContain('var(--state-paused)');
    expect(dot('ended')).toContain('var(--state-closed)');
  });
});
