import { describe, expect, it } from 'vitest';
import { isDone } from './task-status';
import { autoSummary, blockers, byId, chainOf, depCandidates, startable, flagPatch, isOrganizer, matrix, mergeMessage, organizing, quadrant, quadrantFlags, canDrop, cardMeta, modelLabel, moveAction, cardStatus, columns, dropTarget, edgeSpeed, insertIndex, isSession, hearsMessages, movableStatuses, moveTask, taskForItem, nextUp, rankAt, relTime, revertTask, shiftStatus, sortColumn, shownCount, grown, insertBefore, CARD_PAGE, newTaskId } from './kanban';
import type { Task, TaskMessage } from '../types';

let n = 0;
const task = (over: Partial<Task>): Task => ({
  id: `t${++n}`,
  number: 100 + n,
  title: 'Task',
  description: '',
  project: 'p',
  status: 'pending',
  agent: 'claude',
  model: 'opus',
  effort: 'high',
  rank: n,
  station: 's',
  createdAt: '2026-10-01T00:00:00Z',
  updatedAt: '2026-10-01T00:00:00Z',
  ...over,
});

describe('columns', () => {
  const tasks = [
    task({ id: 'a', number: 3, rank: 2, status: 'pending', title: 'Rate limit', project: 'api' }),
    task({ id: 'b', number: 1, rank: 1, status: 'pending', title: 'Dark mode', project: 'web' }),
    task({ id: 'c', number: 9, rank: 5, status: 'completed' }),
    task({ id: 'd', number: 8, rank: 5, status: 'completed' }),
  ];

  it('returns the six columns in order, each sorted by rank then number', () => {
    const cols = columns(tasks);
    expect(cols.map((c) => c.status)).toEqual(['pending', 'auto', 'in_progress', 'in_testing', 'merged', 'completed']);
    expect(cols[0].tasks.map((t) => t.id)).toEqual(['b', 'a']);
    expect(cols[5].tasks.map((t) => t.id)).toEqual(['d', 'c']);
  });

  it('puts merged tasks in their own column, apart from closed ones', () => {
    const cols = columns([task({ id: 'm', status: 'merged' }), task({ id: 'x', status: 'completed' })]);
    expect(cols.find((c) => c.status === 'merged')!.tasks.map((t) => t.id)).toEqual(['m']);
    expect(cols.find((c) => c.status === 'completed')!.tasks.map((t) => t.id)).toEqual(['x']);
  });

  it('filters by project and search words', () => {
    expect(columns(tasks, { project: 'web' })[0].tasks.map((t) => t.id)).toEqual(['b']);
    expect(columns(tasks, { search: 'rate LIMIT' })[0].tasks.map((t) => t.id)).toEqual(['a']);
    expect(columns(tasks, { search: '#3' })[0].tasks.map((t) => t.id)).toEqual(['a']);
  });
});

describe('auto queue', () => {
  const tasks = [
    task({ id: 'r', status: 'in_progress' }),
    task({ id: 'q2', number: 20, rank: 2, status: 'auto' }),
    task({ id: 'q1', number: 30, rank: -1, status: 'auto' }),
    task({ id: 'q3', number: 10, rank: 3, status: 'auto' }),
  ];

  it('counts running and queued', () => {
    expect(autoSummary(tasks)).toEqual({ running: 1, queued: 3 });
  });

  it('marks the first free slots as next up, at least one', () => {
    expect([...nextUp(tasks, 3)]).toEqual(['q1', 'q2']);
    expect([...nextUp(tasks, 1)]).toEqual(['q1']);
  });
});

describe('dependencies', () => {
  const tasks = [
    task({ id: 'r', number: 3, status: 'in_progress' }),
    task({ id: 'd1', number: 12, status: 'in_testing', title: 'Schema migration' }),
    task({ id: 'd2', number: 14, status: 'completed', title: 'API contract' }),
    task({ id: 'b', number: 15, rank: 1, status: 'auto', dependsOn: ['d1', 'd2', 'gone'] }),
    task({ id: 'f', number: 16, rank: 2, status: 'auto', dependsOn: ['d2'] }),
    task({ id: 'p', number: 17, status: 'pending', dependsOn: ['b'], project: 'other', title: 'Docs' }),
  ];
  const ids = byId(tasks);

  it('lists only the dependencies that are still open', () => {
    expect(blockers(tasks[3], ids).map((t) => t.number)).toEqual([12]);
    expect(blockers(tasks[4], ids)).toEqual([]);
  });

  it('skips blocked tasks when picking what runs next', () => {
    expect(startable(tasks).map((t) => t.id)).toEqual(['f']);
    expect([...nextUp(tasks, 3)]).toEqual(['f']);
  });

  it('labels blocked cards with the tasks they wait on', () => {
    expect(cardStatus(tasks[3], new Set(['b']), ids)).toEqual({ kind: 'blocked', waiting: [12] });
    expect(cardStatus(tasks[5], new Set(), ids)).toEqual({ kind: 'blocked', waiting: [15] });
    expect(cardStatus(tasks[4], new Set(['f']), ids).kind).toBe('next');
    expect(cardStatus(task({ status: 'in_testing', dependsOn: ['d1'] }), new Set(), ids).kind).toBe('none');
  });

  it('collects the whole chain up and down', () => {
    expect([...chainOf('b', tasks)].sort()).toEqual(['b', 'd1', 'd2', 'gone', 'p']);
    expect([...chainOf('r', tasks)]).toEqual(['r']);
  });

  it('filters picker candidates by number, title and project', () => {
    expect(depCandidates(tasks, '#1', { exclude: ['b'] }).map((t) => t.number)).toEqual([17, 16, 12, 14]);
    expect(depCandidates(tasks, '12').map((t) => t.id)).toEqual(['d1']);
    expect(depCandidates(tasks, 'schema MIG').map((t) => t.id)).toEqual(['d1']);
    expect(depCandidates(tasks, '', { project: 'other' }).map((t) => t.id)).toEqual(['p']);
    expect(depCandidates(tasks, '', { limit: 2 }).map((t) => t.id)).toEqual(['p', 'f']);
    expect(depCandidates(tasks, 'api').map((t) => t.id)).toEqual(['d2']);
  });
});

describe('status from an open tab', () => {
  it('finds the task by line or term and offers the statuses it accepts', () => {
    const a = task({ id: 'a', lineId: 'l1' });
    const b = task({ id: 'b', termId: 't2', origin: 'session' });
    expect(taskForItem([a, b], { lineId: 'l1' })?.id).toBe('a');
    expect(taskForItem([a, b], { termId: 't2' })?.id).toBe('b');
    expect(taskForItem([a, b], { lineId: 'zz' })).toBeUndefined();
    expect(taskForItem([a, b], undefined)).toBeUndefined();
    expect(movableStatuses(a)).toEqual(['in_testing', 'merged', 'completed']);
    expect(movableStatuses(b)).toEqual(['in_progress', 'in_testing', 'merged', 'completed']);
  });
});

describe('merged and closed are both done', () => {
  it('treats merged and completed as done and every other status as open', () => {
    expect(isDone('merged')).toBe(true);
    expect(isDone('completed')).toBe(true);
    for (const s of ['pending', 'auto', 'in_progress', 'in_testing'] as const) expect(isDone(s)).toBe(false);
  });

  it('no longer blocks a dependent once its dependency is merged', () => {
    const dep = task({ id: 'dep', status: 'merged' });
    const open = task({ id: 'open', status: 'in_testing' });
    const waiting = task({ id: 'w', status: 'auto', dependsOn: ['dep', 'open'] });
    const ids = byId([dep, open, waiting]);
    expect(blockers(waiting, ids).map((t) => t.id)).toEqual(['open']);
    open.status = 'merged';
    expect(blockers(waiting, byId([dep, open, waiting]))).toEqual([]);
    expect(startable([dep, open, waiting]).map((t) => t.id)).toEqual(['w']);
  });

  it('leaves merged tasks out of the matrix and sorts them after open ones as dependencies', () => {
    const m = task({ id: 'm', number: 900, title: 'Merged dep', status: 'merged', urgent: true, important: true });
    const o = task({ id: 'o', number: 1, title: 'Open dep', status: 'pending', urgent: true, important: true });
    expect(matrix([m, o]).do.map((t) => t.id)).toEqual(['o']);
    expect(depCandidates([m, o], 'dep').map((t) => t.id)).toEqual(['o', 'm']);
  });

  it('shows no card status on a merged session task', () => {
    expect(cardStatus(task({ status: 'merged', origin: 'session', badges: ['needs_input'] }), new Set())).toEqual({ kind: 'none' });
  });

  it('moves a session task to merged with a patch and to closed with complete', () => {
    expect(moveAction('in_testing', 'merged', true)).toBe('patch');
    expect(moveAction('in_testing', 'completed', true)).toBe('complete');
    expect(moveAction('in_testing', 'merged')).toBe('patch');
    expect(moveAction('merged', 'in_progress')).toBe('reject');
    expect(moveAction('completed', 'in_progress')).toBe('start');
    expect(canDrop(task({ origin: 'session' }), 'merged')).toBe(true);
  });
});

describe('cardStatus', () => {
  it('labels starting, next up and progress', () => {
    expect(cardStatus(task({ status: 'in_progress', progress: { done: 0, total: 5 } }), new Set()).kind).toBe('starting');
    expect(cardStatus(task({ status: 'in_progress' }), new Set()).kind).toBe('starting');
    expect(cardStatus(task({ status: 'in_progress', termId: 't1' }), new Set()).kind).toBe('working');
    expect(cardStatus(task({ id: 'x', status: 'auto' }), new Set(['x'])).kind).toBe('next');
    expect(cardStatus(task({ status: 'in_progress', progress: { done: 2, total: 5, note: 'Tests' } }), new Set())).toEqual({ kind: 'progress', pct: 40, note: 'Tests' });
    expect(cardStatus(task({ status: 'completed', progress: { done: 5, total: 5 } }), new Set()).kind).toBe('none');
    expect(cardStatus(task({ status: 'in_testing', progress: { done: 5, total: 5 } }), new Set()).kind).toBe('none');
  });
});

describe('moves', () => {
  it('clamps keyboard column shifts', () => {
    expect(shiftStatus('pending', -1)).toBe('pending');
    expect(shiftStatus('pending', 1)).toBe('auto');
    expect(shiftStatus('completed', 1)).toBe('completed');
  });

  it('resolves a drop column from pointer x', () => {
    const rects = [
      { status: 'pending' as const, left: 0, right: 100 },
      { status: 'auto' as const, left: 110, right: 210 },
    ];
    expect(dropTarget(50, rects)).toBe('pending');
    expect(dropTarget(105, rects)).toBe('pending');
    expect(dropTarget(108, rects)).toBe('auto');
    expect(dropTarget(400, rects)).toBeNull();
    expect(dropTarget(10, [])).toBeNull();
  });

  it('moves optimistically and reverts', () => {
    const tasks = [task({ id: 'm', status: 'pending' }), task({ id: 'o' })];
    const { next, prev } = moveTask(tasks, 'm', 'auto', 'now');
    expect(next[0]).toMatchObject({ status: 'auto', updatedAt: 'now' });
    expect(tasks[0].status).toBe('pending');
    expect(revertTask(next, prev!)).toEqual(tasks);
    expect(moveTask(tasks, 'm', 'pending').prev).toBeNull();
    expect(moveTask(tasks, 'zz', 'auto').prev).toBeNull();
  });
});

describe('ordering', () => {
  const col = [task({ id: 'x', rank: 1 }), task({ id: 'y', rank: 2 }), task({ id: 'z', rank: 4 })];
  const by = (id: string) => col.find((t) => t.id === id)!;

  it('falls back to the number when a task has no rank', () => {
    const legacy = [task({ id: 'l2', number: 2, rank: undefined as unknown as number }), task({ id: 'l1', number: 1, rank: undefined as unknown as number })];
    expect(sortColumn(legacy).map((t) => t.id)).toEqual(['l1', 'l2']);
  });

  it('ranks a drop between neighbours, at the top, at the bottom', () => {
    const outsider = task({ id: 'o', rank: 99, status: 'auto' });
    expect(rankAt(col, 0, outsider)).toBe(0);
    expect(rankAt(col, 1, outsider)).toBe(1.5);
    expect(rankAt(col, 2, outsider)).toBe(3);
    expect(rankAt(col, 3, outsider)).toBe(5);
    expect(rankAt(col, 42, outsider)).toBe(5);
  });

  it('keeps the rank when dropped in place or into an empty column', () => {
    expect(rankAt(col, 1, by('y'))).toBe(2);
    expect(rankAt([], 0, by('y'))).toBe(2);
    expect(rankAt([by('y')], 0, by('y'))).toBe(2);
  });

  it('reorders within a column ignoring the moving card', () => {
    expect(rankAt(col, 0, by('z'))).toBe(0);
    expect(rankAt(col, 2, by('x'))).toBe(5);
    expect(rankAt(col, 1, by('x'))).toBe(3);
  });

  it('finds the insertion index from card midpoints', () => {
    expect(insertIndex(5, [10, 30, 50])).toBe(0);
    expect(insertIndex(31, [10, 30, 50])).toBe(2);
    expect(insertIndex(99, [10, 30, 50])).toBe(3);
    expect(insertIndex(0, [])).toBe(0);
  });

  it('moves with a rank optimistically, and ignores no-op reorders', () => {
    const { next, prev } = moveTask(col, 'z', 'pending', 'now', 0);
    expect(next.find((t) => t.id === 'z')).toMatchObject({ rank: 0, status: 'pending' });
    expect(prev!.rank).toBe(4);
    expect(moveTask(col, 'z', 'pending', 'now', 4).prev).toBeNull();
  });

  it('auto-scrolls faster closer to an edge', () => {
    expect(edgeSpeed(200, 0, 400)).toBe(0);
    expect(edgeSpeed(0, 0, 400)).toBe(-18);
    expect(edgeSpeed(24, 0, 400)).toBe(-9);
    expect(edgeSpeed(400, 0, 400)).toBe(18);
    expect(edgeSpeed(500, 0, 400)).toBe(18);
    expect(edgeSpeed(10, 0, 60)).toBe(0);
  });
});

describe('relTime', () => {
  const now = Date.parse('2026-10-29T12:00:00Z');
  it('buckets ages', () => {
    expect(relTime('2026-10-29T11:59:50Z', now)).toEqual({ key: 'time.now', n: 0 });
    expect(relTime('2026-10-29T11:30:00Z', now)).toEqual({ key: 'time.m', n: 30 });
    expect(relTime('2026-10-29T09:00:00Z', now)).toEqual({ key: 'time.h', n: 3 });
    expect(relTime('2026-10-26T12:00:00Z', now)).toEqual({ key: 'time.d', n: 3 });
  });
});

describe('moveAction', () => {
  it('starts, rejects or patches by contract', () => {
    expect(moveAction('pending', 'in_progress')).toBe('start');
    expect(moveAction('auto', 'in_progress')).toBe('start');
    expect(moveAction('in_testing', 'in_progress')).toBe('reject');
    expect(moveAction('in_progress', 'in_testing')).toBe('patch');
    expect(moveAction('auto', 'auto')).toBeNull();
  });

  it('never starts a session task: patches it, and completing it holds the session', () => {
    expect(moveAction('in_testing', 'in_progress', true)).toBe('patch');
    expect(moveAction('in_progress', 'in_testing', true)).toBe('patch');
    expect(moveAction('in_progress', 'completed', true)).toBe('complete');
    expect(moveAction('in_progress', 'pending', true)).toBeNull();
    expect(moveAction('in_progress', 'auto', true)).toBeNull();
  });
});

describe('session tasks', () => {
  const s = (over: Partial<Task>) => task({ origin: 'session', lineId: 'ln', status: 'in_progress', ...over });

  it('are recognised by their origin', () => {
    expect(isSession(s({}))).toBe(true);
    expect(isSession(task({}))).toBe(false);
    expect(isSession(task({ origin: 'user' }))).toBe(false);
  });

  it('show working, progress, needs input and paused variants', () => {
    expect(cardStatus(s({}), new Set())).toEqual({ kind: 'working' });
    expect(cardStatus(s({ progress: { done: 1, total: 4, note: 'Tests' } }), new Set())).toEqual({ kind: 'progress', pct: 25, note: 'Tests' });
    expect(cardStatus(s({ badges: ['needs_input'], progress: { done: 1, total: 4 } }), new Set()).kind).toBe('input');
    expect(cardStatus(s({ badges: ['paused'] }), new Set()).kind).toBe('paused');
    expect(cardStatus(s({ status: 'in_testing', badges: ['paused'] }), new Set()).kind).toBe('paused');
    expect(cardStatus(s({ status: 'in_testing' }), new Set()).kind).toBe('none');
    expect(cardStatus(s({ status: 'completed', badges: ['needs_input'] }), new Set()).kind).toBe('none');
  });

  it('can be hidden by the filter', () => {
    const tasks = [s({ id: 'x' }), task({ id: 'y', status: 'in_progress' })];
    expect(columns(tasks)[2].tasks.map((t) => t.id).sort()).toEqual(['x', 'y']);
    expect(columns(tasks, { hideSession: true })[2].tasks.map((t) => t.id)).toEqual(['y']);
    expect(columns(tasks, { hideSession: false, search: 'task' })[2].tasks.length).toBe(2);
  });

  it('only drop into the columns a session can be in', () => {
    expect(canDrop(s({}), 'pending')).toBe(false);
    expect(canDrop(s({}), 'auto')).toBe(false);
    expect(canDrop(s({}), 'in_testing')).toBe(true);
    expect(canDrop(s({}), 'completed')).toBe(true);
    expect(canDrop(task({}), 'auto')).toBe(true);
  });
});

describe('card meta', () => {
  const session = (over: Partial<Task>) => task({ origin: 'session', status: 'in_progress', project: '/w/app', ...over });

  it('shows the project only on the all-projects board', () => {
    expect(cardMeta(session({}), { filtered: false }).project).toBe(true);
    expect(cardMeta(session({}), { filtered: true }).project).toBe(false);
  });

  it('hides the default branch and keeps feature branches', () => {
    expect(cardMeta(session({ branch: 'main' }), { filtered: true, defaultBranch: 'main' }).branch).toBeNull();
    expect(cardMeta(session({ branch: 'master' }), { filtered: true }).branch).toBeNull();
    expect(cardMeta(session({ branch: 'develop' }), { filtered: true, defaultBranch: 'develop' }).branch).toBeNull();
    expect(cardMeta(session({ branch: 'main' }), { filtered: true, defaultBranch: 'develop' }).branch).toBe('main');
    expect(cardMeta(session({ branch: 'feat/x' }), { filtered: true, defaultBranch: 'main' }).branch).toBe('feat/x');
    expect(cardMeta(session({ branch: null }), { filtered: true }).branch).toBeNull();
  });

  it('names the worktree when it is not the checkout or the branch', () => {
    expect(cardMeta(session({ worktreePath: '/w/app' }), { filtered: true }).worktree).toBeNull();
    expect(cardMeta(session({ worktreePath: '/w/app/.claude/worktrees/card-names', branch: 'fix/session-task-names' }), { filtered: true }).worktree).toBe('card-names');
    expect(cardMeta(session({ worktreePath: '/w/app/.claude/worktrees/relay', branch: 'feat/relay' }), { filtered: true }).worktree).toBeNull();
  });

  it('passes the doing-now note through for working sessions', () => {
    expect(cardStatus(session({ progress: { done: 0, total: 0, note: 'Editing src/daemon.rs' } }), new Set())).toEqual({ kind: 'working', note: 'Editing src/daemon.rs' });
  });

  it('labels models', () => {
    expect(modelLabel('claude-opus-5-5')).toBe('Opus 5.5');
    expect(modelLabel('claude-sonnet-4-5-20250929')).toBe('Sonnet 4.5');
    expect(modelLabel('opus')).toBe('Opus');
    expect(modelLabel('claude-opus-4-1[1m]')).toBe('Opus 4.1');
    expect(modelLabel('claude-3-5-haiku-20241022')).toBe('Haiku 3.5');
    expect(modelLabel('gpt-5-codex')).toBe('gpt-5-codex');
    expect(modelLabel('')).toBe('');
  });
});

describe('quadrant', () => {
  it('derives the Eisenhower quadrant from the two flags', () => {
    expect(quadrant({ urgent: true, important: true })).toBe('do');
    expect(quadrant({ urgent: false, important: true })).toBe('plan');
    expect(quadrant({ urgent: true, important: false })).toBe('delegate');
    expect(quadrant({ urgent: false, important: false })).toBe('drop');
  });

  it('is unclassified when either flag is unset', () => {
    expect(quadrant({})).toBe('none');
    expect(quadrant({ urgent: true })).toBe('none');
    expect(quadrant({ urgent: null, important: true })).toBe('none');
  });

  it('round-trips through quadrantFlags', () => {
    for (const q of ['do', 'plan', 'delegate', 'drop'] as const) expect(quadrant(quadrantFlags(q))).toBe(q);
    expect(quadrantFlags('none')).toEqual({ urgent: null, important: null });
  });

  it('flagPatch fills the other flag with false', () => {
    expect(flagPatch({}, 'urgent', true)).toEqual({ urgent: true, important: false });
    expect(flagPatch({ urgent: true, important: true }, 'urgent', false)).toEqual({ urgent: false, important: true });
  });
});

describe('matrix', () => {
  const tasks = [
    task({ id: 'a', rank: 3, urgent: true, important: true, project: 'api' }),
    task({ id: 'b', rank: 1, urgent: true, important: true, project: 'api' }),
    task({ id: 'c', rank: 1, important: true, project: 'web' }),
    task({ id: 'd', rank: 2, urgent: false, important: false }),
    task({ id: 'e', rank: 2, urgent: true, important: false, status: 'in_progress' }),
    task({ id: 'f', rank: 2 }),
    task({ id: 'g', urgent: true, important: true, status: 'completed' }),
    task({ id: 'h', urgent: true, important: true, origin: 'organizer' }),
    task({ id: 'i', urgent: true, important: true, origin: 'session', status: 'in_progress' }),
  ];

  it('groups by quadrant, sorted by rank, skipping completed, organizer and session tasks', () => {
    const m = matrix(tasks);
    expect(m.do.map((x) => x.id)).toEqual(['b', 'a']);
    expect(m.plan).toEqual([]);
    expect(m.delegate.map((x) => x.id)).toEqual(['e']);
    expect(m.drop.map((x) => x.id)).toEqual(['d']);
    expect(m.none.map((x) => x.id)).toEqual(['c', 'f']);
  });

  it('applies the board filter', () => {
    const m = matrix(tasks, { project: 'api' });
    expect(m.do.map((x) => x.id)).toEqual(['b', 'a']);
    expect(m.none).toEqual([]);
  });
});

describe('organizer tasks', () => {
  const org = task({ origin: 'organizer', status: 'in_progress', project: 'api' });

  it('are hidden from the columns and not counted as running', () => {
    expect(columns([org]).flatMap((c) => c.tasks)).toEqual([]);
    expect(autoSummary([org, task({ status: 'in_progress' })]).running).toBe(1);
    expect(isOrganizer(org)).toBe(true);
  });

  it('organizing reports a running organizer for the project filter', () => {
    expect(organizing([org])).toBe(true);
    expect(organizing([org], 'api')).toBe(true);
    expect(organizing([org], 'web')).toBe(false);
    expect(organizing([{ ...org, status: 'completed' }])).toBe(false);
  });
});

describe('mergeMessage', () => {
  const msg = (id: string, at: string, over: Partial<TaskMessage> = {}): TaskMessage => ({ id, taskId: 't', from: null, body: id, createdAt: at, delivered: false, ...over });

  it('adds new messages in chronological order', () => {
    const a = msg('a', '2026-10-01T00:00:02Z');
    const b = msg('b', '2026-10-01T00:00:01Z');
    expect(mergeMessage(mergeMessage([], a), b).map((m) => m.id)).toEqual(['b', 'a']);
  });

  it('ignores duplicates and updates the delivered flag', () => {
    const a = msg('a', '2026-10-01T00:00:01Z');
    const list = [a];
    expect(mergeMessage(list, { ...a })).toBe(list);
    expect(mergeMessage(list, { ...a, delivered: true })[0].delivered).toBe(true);
  });
});

describe('card windowing', () => {
  it('renders one page first and never more than the column', () => {
    expect(shownCount(undefined, 300)).toBe(CARD_PAGE);
    expect(shownCount(undefined, 3)).toBe(3);
    expect(shownCount(50, 40)).toBe(40);
  });

  it('always includes the card being moved', () => {
    expect(shownCount(undefined, 300, 120)).toBe(121);
    expect(shownCount(undefined, 300, 2)).toBe(CARD_PAGE);
  });

  it('grows by two pages up to the total', () => {
    expect(grown(undefined, 300)).toBe(CARD_PAGE * 3);
    expect(grown(CARD_PAGE * 3, 300)).toBe(CARD_PAGE * 5);
    expect(grown(290, 300)).toBe(300);
  });
});

describe('newTaskId', () => {
  const list = ['a', 'b', 'c'].map((id) => task({ id }));

  it('ignores the first load', () => {
    expect(newTaskId(null, list)).toBeNull();
  });

  it('returns nothing when no task appeared', () => {
    expect(newTaskId(new Set(['a', 'b', 'c']), list)).toBeNull();
  });

  it('returns the task that appeared', () => {
    expect(newTaskId(new Set(['a', 'b']), list)).toBe('c');
  });

  it('returns the last one when several appeared', () => {
    expect(newTaskId(new Set(['a']), list)).toBe('c');
  });

  it('puts a task created behind a full page inside the window', () => {
    const many = Array.from({ length: CARD_PAGE + 3 }, (_, i) => task({ id: `n${i}` }));
    const added = newTaskId(new Set(many.slice(0, -1).map((x) => x.id)), many)!;
    const at = many.findIndex((x) => x.id === added);
    expect(shownCount(undefined, many.length, at)).toBe(many.length);
  });
});

describe('insertBefore', () => {
  const col = ['a', 'b', 'c', 'd'].map((id) => task({ id }));

  it('returns the card the marker sits above, skipping the moving card', () => {
    expect(insertBefore(col, null, 0)).toBe('a');
    expect(insertBefore(col, 'a', 0)).toBe('b');
    expect(insertBefore(col, 'b', 1)).toBe('c');
  });

  it('returns null when the marker goes after the last card', () => {
    expect(insertBefore(col, 'b', 3)).toBeNull();
    expect(insertBefore([], null, 0)).toBeNull();
  });
});

describe('hearsMessages', () => {
  const t = (status: Task['status'], termId?: string) => ({ status, termId }) as Task;
  const terms = [{ id: 'live', exited: false }, { id: 'gone', exited: true }] as { id: string; exited?: boolean }[];

  it('is true for a running task whose terminal is live', () => {
    expect(hearsMessages(t('in_progress', 'live'), terms)).toBe(true);
  });

  it('is true for a task waiting in Review with a live terminal, as the daemon delivers to it', () => {
    expect(hearsMessages(t('in_testing', 'live'), terms)).toBe(true);
  });

  it('is false without a terminal, with an ended terminal or an unknown one', () => {
    expect(hearsMessages(t('in_testing'), terms)).toBe(false);
    expect(hearsMessages(t('in_progress', 'gone'), terms)).toBe(false);
    expect(hearsMessages(t('in_progress', 'missing'), terms)).toBe(false);
  });
});
