import { describe, expect, it } from 'vitest';
import { api } from './api';
import { broadcastBody, summarize, targetRows } from './broadcast';
import type { SidebarItem } from './grouping';
import { MockDaemon } from './mock';
import { stateOfGroup } from './test-fixtures';
import { idleTransitions } from './notify';
import type { DaemonEvent } from './types';

const item = (key: string, group: SidebarItem['group'], over: Partial<SidebarItem> = {}): SidebarItem => ({
  key,
  kind: 'claude',
  group,
  state: stateOfGroup(group),
  title: key,
  at: '',
  lineId: key.startsWith('line:') ? key.slice(5) : undefined,
  termId: key.startsWith('term:') ? key.slice(5) : undefined,
  ...over,
});

describe('broadcast dialog', () => {
  const items = [item('line:busy', 'running'), item('line:idle', 'ready'), item('line:ask', 'needs_input'), item('line:paused', 'paused'), item('line:off', 'stopped'), item('term:sh', 'running', { kind: 'shell' })];

  it('checks idle agents by default, never shells, paused or stopped lines', () => {
    const rows = targetRows(items, false);
    expect(rows.map((r) => [r.key, r.state, r.selectable, r.checked])).toEqual([
      ['line:idle', 'idle', true, true],
      ['term:sh', 'idle', true, false],
      ['line:ask', 'needs_input', false, false],
      ['line:busy', 'busy', false, false],
      ['line:paused', 'paused', false, false],
      ['line:off', 'stopped', false, false],
    ]);
  });

  it('lets busy agents in when queueing', () => {
    const rows = targetRows(items, true);
    expect(rows.filter((r) => r.checked).map((r) => r.key)).toEqual(['line:idle', 'line:ask', 'line:busy']);
  });

  it('sends explicit line and term ids with the safe defaults', () => {
    expect(broadcastBody(['line:idle', 'term:sh', 'line:nope'], items, '  rebase  ', true)).toEqual({ text: 'rebase', lineIds: ['idle'], termIds: ['sh'], onlyIdle: true, queue: true });
    expect(summarize([{ target: 'a', result: 'sent' }, { target: 'b', result: 'queued' }, { target: 'c', result: 'refused', reason: 'not running' }])).toEqual({ sent: 1, queued: 1, skipped: 0, refused: 1 });
  });

  it('notifies once when a watched scope becomes idle', () => {
    const before = new Map([['w:a', false], ['w:b', true]]);
    expect(idleTransitions(before, new Map([['w:a', true], ['w:b', true], ['w:c', true]]))).toEqual(['w:a']);
  });
});

describe('mock daemon workspaces and broadcast', () => {
  it('serves the folders and workspaces of the example and persists edits', async () => {
    const m = new MockDaemon({ live: false });
    const a = api(m);
    const events: DaemonEvent[] = [];
    m.subscribe((e) => events.push(e));
    expect((await a.folders()).map((f) => f.name)).toEqual(['Uni', 'alice', 'acme']);
    expect((await a.workspaces())[0]).toMatchObject({ name: 'webshop', pinned: true });
    const w = await a.createWorkspace({ path: '~/notes', folderId: 'f_alice' });
    expect(w).toMatchObject({ path: '/home/alice/notes', name: 'notes', auto: false });
    expect((await a.createWorkspace({ path: '/home/alice/notes' })).id).toBe(w.id);
    await a.patchWorkspace(w.id, { tabs: ['term:x'], pinned: true });
    await expect(a.createFolder({ name: 'deep', parentId: (await a.createFolder({ name: 'sub', parentId: 'f_alice' })).id })).rejects.toMatchObject({ status: 400 });
    const g = await a.createGroup({ name: 'release 5.9', members: ['line:ln_01'] });
    expect((await a.patchGroup(g.id, { add: ['line:ln_02'], remove: ['line:ln_01'] })).members).toEqual(['line:ln_02']);
    await a.deleteFolder('f_alice');
    expect((await a.workspaces()).find((x) => x.id === w.id)?.folderId).toBeNull();
    expect(events.map((e) => e.type)).toContain('workspace.updated');
    expect(events.map((e) => e.type)).toContain('folder.deleted');
  });

  it('sends to idle agents, skips or queues busy ones and delivers the queue when they finish', async () => {
    const m = new MockDaemon({ live: false });
    const a = api(m);
    const events: DaemonEvent[] = [];
    m.subscribe((e) => events.push(e));
    const r = await a.broadcast({ text: 'pull main', lineIds: ['ln_09', 'ln_04', 'ln_11'] });
    expect(r.results).toEqual([
      { target: 'line:ln_09', result: 'sent' },
      { target: 'line:ln_04', result: 'skipped', reason: 'busy' },
      { target: 'line:ln_11', result: 'refused', reason: 'not running' },
    ]);
    const q = await a.broadcast({ text: 'pull main', groupId: 'g_infra', queue: true });
    expect(q.results.find((x) => x.target === 'line:ln_04')).toMatchObject({ result: 'queued' });
    m.lines.find((l) => l.line.id === 'ln_04')!.local!.busy = false;
    m.drainQueue();
    expect(events.find((e) => e.type === 'broadcast.delivered')?.data).toMatchObject({ target: 'line:ln_04', result: 'sent' });
    await expect(a.broadcast({ text: '  ', lineIds: ['ln_09'] })).rejects.toMatchObject({ status: 400 });
    expect((await a.broadcast({ text: 'x', workspaceId: 'ws_webshop' })).results.length).toBe(4);
  });

  it('previews a hold without acting and reports refused checks', async () => {
    const m = new MockDaemon({ live: false });
    const a = api(m);
    expect(await a.holdPlan('ln_09')).toMatchObject({ lineId: 'ln_09', predicates: { alive: { ok: true } } });
    expect((await a.holdPlan('ln_04')).error).toBe('turn in progress');
    expect(m.lines.find((l) => l.line.id === 'ln_09')!.line.state).toBe('idle');
  });
});
