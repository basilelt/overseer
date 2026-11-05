import { describe, expect, it } from 'vitest';
import { checkedIds, defaultChecked, describe as describeAction, grouped, initialChecks, nudgeVisible, NUDGE_THRESHOLD, sectionOf, tally, unplacedCount, type Action } from './organize';
import type { LocalLine, Project, Task, Workspace } from './types';

const NOW = Date.parse('2026-10-30T12:00:00Z');

const line = (id: string, worktreePath: string, ago = 60_000, state: LocalLine['line']['state'] = 'idle', repo = ''): LocalLine => ({
  line: { id, agent: 'claude', worktreePath, repo, state, lastActivityAt: new Date(NOW - ago).toISOString() },
});

const ws = (id: string, path: string, archived = false): Workspace => ({
  id,
  name: id,
  path,
  folderId: null,
  pinned: false,
  archived,
  auto: false,
  rank: 0,
  tabs: [],
  notifyIdle: false,
  createdAt: '',
  updatedAt: '',
});

const actions: Action[] = [
  { id: 'a1', kind: 'create_folder', name: 'Acme', ref: 'f1' },
  { id: 'a2', kind: 'hold', lineId: 'l1' },
  { id: 'a3', kind: 'create_workspace', path: '/src/app', folder: 'f1' },
  { id: 'a4', kind: 'close', lineId: 'l2' },
  { id: 'a5', kind: 'set_task_status', taskId: 't1', status: 'completed' },
  { id: 'a6', kind: 'move_workspace', workspaceId: 'w1', folder: 'f0' },
];

describe('nudge', () => {
  it('counts recent lines that belong to no workspace', () => {
    const lines = [
      line('in', '/home/u/app/x'),
      line('out', '/srv/x'),
      line('old', '/srv/y', 30 * 86_400_000),
      line('gone', '/srv/z', 1000, 'hung-up'),
      line('repo', '/tmp/wt', 1000, 'idle', 'github.com/a/lib'),
      line('arch', '/opt/arch/x'),
    ];
    const projects: Project[] = [{ path: '/home/u/lib', name: 'lib', repo: 'github.com/a/lib' }];
    const workspaces = [ws('w1', '/home/u/app'), ws('w2', '/home/u/lib'), ws('w3', '/opt/arch', true)];
    expect(unplacedCount(lines, workspaces, projects, [] as Task[], NOW)).toBe(2);
  });

  it('shows above the threshold until dismissed, and again when the count grows', () => {
    expect(nudgeVisible(NUDGE_THRESHOLD, undefined)).toBe(false);
    expect(nudgeVisible(NUDGE_THRESHOLD + 1, undefined)).toBe(true);
    expect(nudgeVisible(20, 20)).toBe(false);
    expect(nudgeVisible(20 + NUDGE_THRESHOLD + 1, 20)).toBe(true);
  });
});

describe('plan checklist', () => {
  it('checks everything except close by default', () => {
    expect(actions.filter(defaultChecked).map((a) => a.id)).toEqual(['a1', 'a2', 'a3', 'a5', 'a6']);
  });

  it('turns the checklist into action ids in proposal order', () => {
    const c = initialChecks(actions);
    c.a4 = true;
    c.a1 = false;
    expect(checkedIds(actions, c)).toEqual(['a2', 'a3', 'a4', 'a5', 'a6']);
    expect(checkedIds(actions, {})).toEqual([]);
  });

  it('groups actions by section and drops empty sections', () => {
    expect(grouped(actions).map((s) => [s.id, s.actions.map((a) => a.id)])).toEqual([
      ['layout', ['a1', 'a3', 'a6']],
      ['cleanup', ['a2', 'a4']],
      ['tasks', ['a5']],
    ]);
    expect(grouped([actions[1]]).map((s) => s.id)).toEqual(['cleanup']);
    expect(sectionOf('track')).toBe('tasks');
    expect(sectionOf('pin')).toBe('layout');
  });

  it('names actions from refs, folders, lines and tasks', () => {
    const names = {
      folders: [{ id: 'f0', name: 'School' }],
      workspaces: [{ id: 'w1', name: 'thesis' }],
      lines: [{ line: { id: 'l1', agent: 'claude', state: 'idle', lastActivityAt: '' }, local: { title: 'Fix login' } }] as LocalLine[],
      tasks: [{ id: 't1', title: 'VAT rounding' }] as Task[],
    };
    expect(describeAction(actions[2], actions, names)).toEqual({ key: 'org.kind.create_workspace', params: { name: 'app', folder: 'Acme', n: 0, status: '' } });
    expect(describeAction(actions[5], actions, names).params).toMatchObject({ name: 'thesis', folder: 'School' });
    expect(describeAction(actions[1], actions, names).params.name).toBe('Fix login');
    expect(describeAction(actions[3], actions, names).params.name).toBe('l2');
    expect(describeAction(actions[4], actions, names).params).toMatchObject({ name: 'VAT rounding', status: 'completed' });
    expect(describeAction({ ...actions[1], label: 'Given' }, actions, names).params.name).toBe('Given');
  });

  it('tallies results', () => {
    expect(
      tally([
        { actionId: 'a', kind: 'hold', status: 'done' },
        { actionId: 'b', kind: 'hold', status: 'skipped' },
        { actionId: 'c', kind: 'close', status: 'failed', detail: 'busy' },
        { actionId: 'd', kind: 'pin', status: 'done' },
      ]),
    ).toEqual({ done: 2, skipped: 1, failed: 1 });
  });
});
