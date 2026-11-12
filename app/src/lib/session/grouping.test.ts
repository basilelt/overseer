import { describe, expect, it } from 'vitest';
import { GROUP_ORDER, groupItems, lineGroup, sidebarItems, stableItems } from './grouping';
import { cleanTitle, titleEdit } from '../tabs/titles';
import type { LocalLine, Task, Term } from '../types';

const line = (id: string, over: Partial<LocalLine['line']> = {}, local: LocalLine['local'] = {}): LocalLine => ({
  line: { id, agent: 'claude', state: 'active', lastActivityAt: '2026-10-29T10:00:00Z', ...over },
  local,
});

describe('lineGroup', () => {
  it('ranks paused over needs input over testing over running over pushed', () => {
    expect(lineGroup(line('a', { state: 'held' }, { needsInput: true, heldReason: 'user' }), true)).toBe('paused');
    expect(lineGroup(line('a', {}, { pid: 1, needsInput: true, busy: true }), true)).toBe('needs_input');
    expect(lineGroup(line('a', {}, { pid: 1, busy: true }), true)).toBe('needs_testing');
    expect(lineGroup(line('a', { prUrl: 'x' }, { pid: 1, busy: true }), false)).toBe('running');
    expect(lineGroup(line('a', { prUrl: 'x' }, { pid: 1 }), false)).toBe('pushed');
    expect(lineGroup(line('a', { state: 'idle' }, { pid: 1 }), false)).toBe('ready');
  });

  it('keeps a pause waiting for the end of the turn in the running group', () => {
    expect(lineGroup(line('a', {}, { pid: 1, busy: true, holdPending: true }), false)).toBe('running');
  });

  it('splits paused on purpose from stopped, whatever the testing or PR state', () => {
    expect(lineGroup(line('a', { state: 'held' }, { heldReason: 'user' }), true)).toBe('paused');
    expect(lineGroup(line('a', { state: 'held' }, { heldReason: 'lost' }), true)).toBe('stopped');
    expect(lineGroup(line('a', { state: 'held' }), false)).toBe('stopped');
  });

  it('is stopped when an active line has no process or terminal, unless a PR or testing keeps it visible', () => {
    expect(lineGroup(line('a'), false)).toBe('stopped');
    expect(lineGroup(line('a'), false, 't1')).toBe('ready');
    expect(lineGroup(line('a', { prUrl: 'x' }), false)).toBe('pushed');
    expect(lineGroup(line('a'), true)).toBe('needs_testing');
  });

  it('drops hung-up lines', () => {
    expect(lineGroup(line('a', { state: 'hung-up' }), false)).toBeNull();
  });
});

describe('sidebarItems feedback sessions', () => {
  const fb = { id: 'f1', lineId: 'fb', exited: false, cmd: ['claude', 'prompt'], title: 'Feedback', cwd: '/x/work', createdAt: '2026-10-29T09:00:00Z' } as Term;
  const wt = { worktreePath: '/x/work' };

  it('titles a feedback session with its term title instead of the cwd basename', () => {
    expect(sidebarItems([line('fb', wt)], [fb], [])[0].title).toBe('Feedback');
  });

  it('keeps a rename of a feedback session', () => {
    expect(sidebarItems([line('fb', wt, { titleOverride: 'Mine' })], [fb], [])[0].title).toBe('Mine');
  });

  it('keeps the cwd basename for a session whose term is not feedback', () => {
    expect(sidebarItems([line('fb', wt)], [{ ...fb, title: 'zsh' }], [])[0].title).toBe('work');
  });
});

describe('sidebarItems', () => {
  const tasks = [{ id: 't1', lineId: 'b', status: 'in_testing', title: 'Task B', description: 'Do B' } as Task];
  const terms = [
    { id: 'x1', lineId: 'a', exited: false, cmd: ['claude'], title: '', cwd: '/', createdAt: '2026-10-29T09:00:00Z' } as Term,
    { id: 's1', exited: false, cmd: ['zsh'], title: 'zsh', cwd: '/tmp', createdAt: '2026-10-29T11:00:00Z' } as Term,
  ];
  const items = sidebarItems([line('a', {}, { busy: true, title: 'A' }), line('b')], terms, tasks, { a: { goal: 'Goal A', status: 'Editing' } });

  it('maps lines and loose terms', () => {
    expect(items.map((i) => [i.key, i.group])).toEqual([
      ['line:a', 'running'],
      ['line:b', 'needs_testing'],
      ['term:s1', 'running'],
    ]);
  });

  it('carries the process state next to the group', () => {
    expect(items.map((i) => [i.key, i.state])).toEqual([
      ['line:a', 'running'],
      ['line:b', 'closed'],
      ['term:s1', 'running'],
    ]);
    expect(sidebarItems([], [{ id: 'x', exited: true, cmd: ['zsh'], title: '', cwd: '/', createdAt: '' } as Term], [])[0]).toMatchObject({ group: 'stopped', state: 'closed' });
  });

  it('carries term, task, goal and status', () => {
    expect(items[0]).toMatchObject({ termId: 'x1', goal: 'Goal A', status: 'Editing', title: 'A' });
    expect(items[1]).toMatchObject({ taskId: 't1', title: 'Task B', goal: 'Do B' });
  });

  it('leaves lines alone when only a session task follows them', () => {
    const own = [{ id: 's', lineId: 'b', status: 'in_testing', title: 'Session B', description: '', origin: 'session' } as Task];
    const b = sidebarItems([line('b')], [], own).find((i) => i.key === 'line:b');
    expect(b?.group).not.toBe('needs_testing');
    expect(b?.taskId).toBeUndefined();
  });

  it('shows the agent of a loose term, a shell otherwise', () => {
    expect(items.find((i) => i.key === 'term:s1')!.kind).toBe('shell');
    const g = { id: 'g1', exited: false, cmd: ['gemini'], title: 'Gemini CLI', cwd: '/w', createdAt: '', agent: 'gemini' } as Term;
    expect(sidebarItems([line('c', { agent: 'copilot' })], [g], [])).toMatchObject([{ kind: 'copilot' }, { kind: 'gemini' }]);
  });

  it('groups in fixed order, newest first', () => {
    const groups = groupItems(items);
    expect(groups.map((g) => g.id)).toEqual(GROUP_ORDER);
    expect(groups.find((g) => g.id === 'running')!.items.map((i) => i.key)).toEqual(['term:s1', 'line:a']);
  });
});

describe('custom titles', () => {
  const task = { id: 't1', lineId: 'a', status: 'in_progress', title: 'Task A' } as Task;

  it('shows a line custom title over the auto and task titles', () => {
    // Given a line with an auto title, a task title and a custom title
    // When the sidebar items are built
    // Then the custom title wins and the item is marked as renamed
    const [it] = sidebarItems([line('a', {}, { title: 'Auto', titleOverride: 'Mine' })], [], [task]);
    expect(it).toMatchObject({ title: 'Mine', renamed: true });
  });

  it('falls back to the auto title once the custom title is cleared', () => {
    // Given a line whose custom title was cleared
    // When the sidebar items are built
    // Then the auto title shows and the item is not marked as renamed
    const [it] = sidebarItems([line('a', {}, { title: 'Auto', titleOverride: null })], [], []);
    expect(it.title).toBe('Auto');
    expect(it.renamed).toBeFalsy();
  });

  it('shows a plain terminal custom title over its auto title', () => {
    // Given a loose terminal with a custom title
    // When the sidebar items are built
    // Then the custom title shows and the item is marked as renamed
    const term = { id: 's1', exited: false, cmd: ['zsh'], title: 'zsh', titleOverride: 'build watch', cwd: '/', createdAt: '' } as Term;
    expect(sidebarItems([], [term], [])[0]).toMatchObject({ title: 'build watch', renamed: true });
  });

  it('normalises a typed title like the daemon does', () => {
    // Given titles with padding, control characters, blanks and excess length
    // When each is cleaned
    // Then it is trimmed, one line, capped at 80 characters, and blank means none
    expect(cleanTitle('  Payments  ')).toBe('Payments');
    expect(cleanTitle('a\nb\tc')).toBe('a b c');
    expect(cleanTitle('   ')).toBeNull();
    expect(cleanTitle('é'.repeat(200))).toHaveLength(80);
  });

  it('turns an inline edit into a rename, a clear or no change', () => {
    // Given an auto-titled item and a renamed item
    // When the inline editor is committed with various drafts
    // Then unchanged drafts do nothing, blanks clear and new text renames
    const auto = { title: 'Auto' };
    const renamed = { title: 'Mine', renamed: true };
    expect(titleEdit(auto, ' Auto ')).toBeUndefined();
    expect(titleEdit(auto, '')).toBeUndefined();
    expect(titleEdit(auto, 'New')).toBe('New');
    expect(titleEdit(renamed, 'Mine')).toBeUndefined();
    expect(titleEdit(renamed, '  ')).toBeNull();
    expect(titleEdit(renamed, 'Other')).toBe('Other');
  });
});

describe('sticky notes on sidebar items', () => {
  const note = (id: string) => ({ id, text: id, createdAt: '', updatedAt: '' });

  it('counts the notes of a session and leaves sessions without notes unmarked', () => {
    // Given a session with two notes and one without
    // When the sidebar items are built
    // Then only the first carries a count
    const [a, b] = sidebarItems([line('a', {}, { pid: 1, notes: [note('1'), note('2')] }), line('b', {}, { pid: 1, notes: [] })], [], []);
    expect(a.notes).toBe(2);
    expect(b.notes).toBeUndefined();
  });

  it('replaces a kept item when its note count changes, so the icon updates', () => {
    // Given items built before and after a note was added
    // When they are stabilised
    // Then the item with a new count is a new object and the list changes
    const before = sidebarItems([line('a', {}, { pid: 1 })], [], []);
    const after = sidebarItems([line('a', {}, { pid: 1, notes: [note('1')] })], [], []);
    const out = stableItems(before, after);
    expect(out).not.toBe(before);
    expect(out[0].notes).toBe(1);
    expect(stableItems(after, sidebarItems([line('a', {}, { pid: 1, notes: [note('9')] })], [], []))).toBe(after);
  });
});
