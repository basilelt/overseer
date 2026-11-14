import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { declOf, parseStyle } from '../../../test/css';
import type { Task } from '../../types';
import TaskCard from './TaskCard.svelte';
import source from './TaskCard.svelte?raw';

const task = (over: Partial<Task> = {}) => ({ id: 't1', number: 1, title: 'Fix login', description: '', status: 'pending', project: 'p', agent: 'claude', model: '', effort: 'medium', rank: 1, station: '', createdAt: '2026-11-05T10:00:00Z', updatedAt: '2026-11-05T10:00:00Z', ...over }) as Task;
const html = (over: Partial<Task> = {}, props: Record<string, unknown> = {}) => render(TaskCard, { props: { task: task(over), color: '#abc', status: { kind: 'none' }, onqueue: () => {}, ondelete: () => {}, ...props } }).body;

describe('TaskCard meta row', () => {
  it('puts the age before the hover actions, both inside one tail', () => {
    const h = html();
    const tail = h.slice(h.indexOf('class="tail'));
    expect(tail.indexOf('class="age')).toBeGreaterThan(-1);
    expect(tail.indexOf('class="age')).toBeLessThan(tail.indexOf('class="actions'));
  });

  it('shows the task cost in the meta row only when the task has one', () => {
    expect(html()).not.toContain('class="cost');
    const h = html({ cost: { cost: 1.5, partial: false } as Task['cost'] });
    expect(h).toMatch(/class="cost mono[^"]*"[^>]*>[^<]*1\.5/);
  });

  it('shows no hover actions on a ghost card', () => {
    expect(html({}, { ghost: true })).not.toContain('class="actions');
  });
});

describe('CSS guard: TaskCard hover actions layout', () => {
  it('keeps the desktop actions in the meta row flow so they never cover chips', () => {
    expect(declOf(source, '.actions', 'position')).not.toBe('absolute');
    expect(declOf(source, '.actions', 'flex')).toBe('none');
  });

  it('never pulls the actions outside the meta row with a negative margin', () => {
    for (const r of parseStyle(source).filter((x) => x.selector === '.actions')) {
      for (const [prop, value] of Object.entries(r.decls)) if (prop.startsWith('margin')) expect(value).not.toMatch(/(^|\s)-\d/);
    }
  });

  it('pushes the age and the actions together to the right edge', () => {
    expect(declOf(source, '.tail', 'margin-left')).toBe('auto');
    expect(declOf(source, '.tail', 'flex')).toBe('none');
  });

  it('lets the branch chip share the first row with the agent instead of taking a row alone', () => {
    expect(declOf(source, '.ref', 'flex')).toBe('1 1 6em');
    expect(declOf(source, '.ref', 'min-width')).toBe('6em');
  });

  it('keeps the effort chip and the cost from shrinking', () => {
    expect(declOf(source, '.effort', 'flex')).toBe('none');
    expect(declOf(source, '.cost', 'flex')).toBe('none');
  });

  it('hides the actions until the card is hovered or focused', () => {
    expect(declOf(source, '.actions', 'opacity')).toBe('0');
    expect(declOf(source, '.card:hover .actions', 'opacity')).toBe('1');
    expect(declOf(source, '.card:focus-within .actions', 'opacity')).toBe('1');
  });
});
