import { describe, expect, it } from 'vitest';
import type { AgentInfo } from '../../types';
import { nextPickable } from './agent-pick';

const agent = (id: string, over: Partial<AgentInfo> = {}): AgentInfo => ({ id, name: id.toUpperCase(), installed: true, efforts: [], resume: false, verified: true, sessions: true, turns: true, ...over });
const absent = (id: string, installable: boolean) => agent(id, { installed: false, installable, install: installable ? 'npm i -g x' : undefined });

describe('keyboard walk over the agents', () => {
  const list = [agent('a'), absent('b', false), absent('c', true), agent('d')];

  it('skips an absent agent that cannot be installed, in both directions, wrapping around', () => {
    expect(nextPickable(list, 0, 1)).toBe(2);
    expect(nextPickable(list, 2, 1)).toBe(3);
    expect(nextPickable(list, 3, 1)).toBe(0);
    expect(nextPickable(list, 0, -1)).toBe(3);
    expect(nextPickable(list, 2, -1)).toBe(0);
  });

  it('reaches the first and the last pickable agent for Home and End', () => {
    expect(nextPickable(list, -1, 1)).toBe(0);
    expect(nextPickable(list, list.length, -1)).toBe(3);
  });

  it('stays where it is when nothing else can be picked', () => {
    expect(nextPickable([absent('x', false), absent('y', false)], 0, 1)).toBe(0);
    expect(nextPickable([], 0, 1)).toBe(0);
  });
});
