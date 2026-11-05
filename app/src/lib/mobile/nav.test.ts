import { describe, expect, it } from 'vitest';
import { nextTab, viewForTab, type NavState } from './nav';

const s = (view: NavState['view'], key: string | null = null): NavState => ({ view, key });

describe('mobile bottom navigation', () => {
  it('follows views opened from elsewhere', () => {
    expect(nextTab('agents', s('agents'), s('kanban'))).toBe('tasks');
    expect(nextTab('tasks', s('kanban'), s('agents', 'line:a'))).toBe('agents');
    expect(nextTab('tasks', s('kanban'), s('agents', 'review:task:t1'))).toBe('review');
  });

  it('follows the selected session', () => {
    expect(nextTab('agents', s('agents', 'line:a'), s('agents', 'review:line:a'))).toBe('review');
    expect(nextTab('review', s('agents', 'review:line:a'), s('agents', 'line:a'))).toBe('agents');
    expect(nextTab('more', s('agents', 'line:a'), s('agents', 'term:t'))).toBe('agents');
    expect(nextTab('agents', s('agents', 'line:a'), s('agents', 'graph:/x'))).toBe('agents');
  });

  it('stays put when nothing changed', () => {
    expect(nextTab('more', s('agents', 'line:a'), s('agents', 'line:a'))).toBe('more');
    expect(nextTab('review', s('agents', 'line:a'), s('agents', null))).toBe('review');
  });

  it('picks the view behind each tab', () => {
    expect(viewForTab('tasks', 'agents')).toBe('kanban');
    expect(viewForTab('agents', 'kanban')).toBe('agents');
    expect(viewForTab('review', 'kanban')).toBe('agents');
    expect(viewForTab('more', 'kanban')).toBe('agents');
    expect(viewForTab('more', 'browser')).toBe('browser');
  });
});
