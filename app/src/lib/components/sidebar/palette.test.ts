import { describe, expect, it } from 'vitest';
import { GROUP_COLOR, STATE_COLOR, WS_COLOR } from '../../workspace/ws-color';
import chatSrc from '../chat/ChatView.svelte?raw';

// Old group/workspace ids that name a session state, and the state they stand for.
const SAME = { needs_input: 'attention', running: 'running', ready: 'open', paused: 'paused', stopped: 'closed' } as const;

describe('one palette for session states', () => {
  it('colours the tab, phone chip, title bar and group dots like the session row', () => {
    for (const [group, state] of Object.entries(SAME)) expect(GROUP_COLOR[group as keyof typeof SAME]).toBe(STATE_COLOR[state]);
  });

  it('colours the workspace and group dots like the session row', () => {
    for (const [ws, state] of Object.entries(SAME)) expect(WS_COLOR[ws as keyof typeof SAME]).toBe(STATE_COLOR[state]);
  });

  it('colours the chat header dot with the state tokens only', () => {
    const css = chatSrc.slice(chatSrc.indexOf('<style'));
    const dot = (cls: string) => css.match(new RegExp(`\\.state\\.${cls} \\{[^}]*\\}`))?.[0] ?? '';
    expect(dot('input')).toContain('var(--state-attention)');
    expect(dot('busy')).toContain('var(--state-running)');
    expect(dot('idle')).toContain('var(--state-open)');
    expect(dot('paused')).toContain('var(--state-paused)');
    expect(dot('ended')).toContain('var(--state-closed)');
  });
});
