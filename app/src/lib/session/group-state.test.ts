import { describe, expect, it } from 'vitest';
import { targetState } from '../broadcast';
import { GROUP_ORDER, GROUP_STATE, STATE_GROUP, type GroupId, type SidebarItem } from './grouping';
import { STATE_URGENCY } from './session-state';

const item = (group: GroupId, kind = 'claude'): SidebarItem => ({ key: 'k', kind, group, state: GROUP_STATE[group], title: 't', at: '' });

describe('group and state table', () => {
  it('maps every group to the process state it implies', () => {
    expect(GROUP_STATE).toEqual({ needs_input: 'attention', needs_testing: 'open', running: 'running', pushed: 'open', ready: 'open', paused: 'paused', stopped: 'closed' });
    expect([...GROUP_ORDER].sort()).toEqual(Object.keys(GROUP_STATE).sort());
  });

  it('gives every state a default group that maps back to it, leaving the testing and pushed overlays out', () => {
    expect(Object.keys(STATE_GROUP).sort()).toEqual([...STATE_URGENCY].sort());
    for (const s of STATE_URGENCY) expect(GROUP_STATE[STATE_GROUP[s]]).toBe(s);
    expect(Object.values(STATE_GROUP)).not.toContain('needs_testing');
    expect(Object.values(STATE_GROUP)).not.toContain('pushed');
  });

  it('maps each group to the broadcast target it implies', () => {
    expect(GROUP_ORDER.map((g) => targetState(item(g)))).toEqual(['needs_input', 'idle', 'busy', 'idle', 'idle', 'paused', 'stopped']);
  });

  it('shows a shell as idle unless it is paused or stopped', () => {
    expect(GROUP_ORDER.map((g) => targetState(item(g, 'shell')))).toEqual(['idle', 'idle', 'idle', 'idle', 'idle', 'paused', 'stopped']);
  });
});
