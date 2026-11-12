import { describe, expect, it } from 'vitest';
import { GROUP_COLOR, STATE_COLOR, WS_COLOR } from '../../workspace/ws-color';
import { declOf } from '../../../test/css';
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

  it('CSS guard: colours the chat header dot with the state tokens', () => {
    const dot = (cls: string) => declOf(chatSrc, `.state.${cls}`, 'background');
    expect(dot('input')).toBe('var(--state-attention)');
    expect(dot('busy')).toBe('var(--state-running)');
    expect(dot('idle')).toBe('var(--state-open)');
    expect(dot('paused')).toBe('var(--state-paused)');
    expect(declOf(chatSrc, '.state.ended', 'box-shadow')).toContain('var(--state-closed)');
  });
});
