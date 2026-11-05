import { describe, expect, it } from 'vitest';
import { claim, retain, same } from './keepalive';

const all = () => true;

describe('terminal keep-alive', () => {
  it('puts the active terminal first and keeps recent ones', () => {
    expect(retain([], 't1', all)).toEqual(['t1']);
    expect(retain(['t1'], 't2', all)).toEqual(['t2', 't1']);
    expect(retain(['t2', 't1'], 't1', all)).toEqual(['t1', 't2']);
  });

  it('keeps warm terminals while a non terminal tab is shown', () => {
    expect(retain(['t1', 't2'], null, all)).toEqual(['t1', 't2']);
  });

  it('drops the least recently used past the bound', () => {
    expect(retain(['t3', 't2', 't1'], 't4', all, 3)).toEqual(['t4', 't3', 't2']);
    expect(retain(['t1', 't2'], 't3', all, 0)).toEqual(['t3']);
  });

  it('drops terminals that exited or moved to another pane, never the active one', () => {
    const keep = (id: string) => id !== 't2';
    expect(retain(['t1', 't2', 't3'], 't2', keep)).toEqual(['t2', 't1', 't3']);
    expect(retain(['t1', 't2', 't3'], 't1', keep)).toEqual(['t1', 't3']);
  });

  it('claims a terminal for the pane that last activated it', () => {
    const owners = new Map<string, number>();
    const live = new Set(['t1', 't2']);
    claim(owners, null, { active: 't1', index: 0 }, live);
    claim(owners, null, { active: 't2', index: 1 }, live);
    claim(owners, { active: 't2', index: 1 }, { active: 't1', index: 1 }, live);
    expect([...owners]).toEqual([
      ['t1', 1],
      ['t2', 1],
    ]);
  });

  it('does not thrash when two panes show the same terminal', () => {
    const owners = new Map<string, number>();
    const live = new Set(['x']);
    const a = { active: 'x', index: 0 };
    const b = { active: 'x', index: 1 };
    claim(owners, null, a, live);
    claim(owners, null, b, live);
    let writes = 0;
    const set = owners.set.bind(owners);
    owners.set = (k, v) => (writes++, set(k, v));
    for (let i = 0; i < 5; i++) {
      claim(owners, a, a, live);
      claim(owners, b, b, live);
    }
    expect(writes).toBe(0);
    expect(owners.get('x')).toBe(1);
  });

  it('forgets exited terminals but keeps the active one', () => {
    const owners = new Map([
      ['gone', 0],
      ['t1', 1],
    ]);
    claim(owners, null, { active: 'new', index: 0 }, new Set(['t1']));
    expect([...owners.keys()].sort()).toEqual(['new', 't1']);
  });

  it('compares lists in order', () => {
    expect(same(['a', 'b'], ['a', 'b'])).toBe(true);
    expect(same(['a', 'b'], ['b', 'a'])).toBe(false);
    expect(same(['a'], ['a', 'b'])).toBe(false);
  });
});
