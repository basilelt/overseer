import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanExits } from './autoclose';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import type { Term } from '../types';
import { allTabs, EMPTY_LAYOUT, workspace } from '../workspace/workspace.svelte';
import { connectMock } from '../../test/connect-mock';
import { quietMock } from '../../test/quiet-mock';

const term = (over: Partial<Term> = {}): Term => ({ id: 'tm_a', pid: 1, cwd: '/', cmd: ['zsh'], title: 'zsh', cols: 80, rows: 24, createdAt: '', exited: false, ...over });

describe('cleanExits', () => {
  it('returns standalone terms that just exited with code 0', () => {
    expect(cleanExits([term()], [term({ exited: true, exitCode: 0 })])).toEqual(['tm_a']);
  });

  it('treats a missing exit code as a clean exit', () => {
    expect(cleanExits([term()], [term({ exited: true })])).toEqual(['tm_a']);
  });

  it('treats an exit after Ctrl+C (130) as a clean exit', () => {
    expect(cleanExits([term()], [term({ exited: true, exitCode: 130 })])).toEqual(['tm_a']);
  });

  it('keeps terms that exit with an error code', () => {
    expect(cleanExits([term()], [term({ exited: true, exitCode: 1 })])).toEqual([]);
  });

  it('keeps an agent term that is not linked to its line yet', () => {
    expect(cleanExits([term({ agent: 'codex' })], [term({ agent: 'codex', exited: true, exitCode: 0 })])).toEqual([]);
  });

  it('still closes a shell term that exits cleanly', () => {
    expect(cleanExits([term({ agent: 'shell' })], [term({ agent: 'shell', exited: true, exitCode: 0 })])).toEqual(['tm_a']);
  });

  it('keeps agent line and task terms', () => {
    expect(cleanExits([term({ lineId: 'ln' }), term({ id: 'tm_b', taskId: 'tk' })], [term({ lineId: 'ln', exited: true }), term({ id: 'tm_b', taskId: 'tk', exited: true })])).toEqual([]);
  });

  it('ignores terms that were already exited or unknown before', () => {
    expect(cleanExits([term({ exited: true })], [term({ exited: true })])).toEqual([]);
    expect(cleanExits([], [term({ exited: true })])).toEqual([]);
  });
});

describe('closing exited terminals', () => {
  let m: MockDaemon;
  let id: string;

  beforeEach(async () => {
    m = await connectMock(quietMock());
    id = app.data.terms.find((t) => !t.lineId && !t.taskId && !t.exited)!.id;
    workspace.layout = { ...EMPTY_LAYOUT, panes: [{ tabs: ['other', `term:${id}`], active: `term:${id}` }] };
    app.selectedKey = `term:${id}`;
  });

  afterEach(async () => {
    await app.disconnect();
    workspace.layout = EMPTY_LAYOUT;
    app.selectedKey = null;
  });

  const exit = (code: number) => m.emit('term.exited', { term: { ...app.data.terms.find((t) => t.id === id)!, exited: true, exitCode: code } });

  it('closes the tab, selects the next one and deletes the term after a clean exit', async () => {
    const request = vi.spyOn(m, 'request');
    exit(0);
    await vi.waitFor(() => expect(allTabs(workspace.layout)).toEqual(['other']), { timeout: 1000, interval: 5 });
    expect(app.selectedKey).toBe('other');
    await vi.waitFor(() => expect(request).toHaveBeenCalledWith('DELETE', `/v1/terms/${encodeURIComponent(id)}`), { timeout: 1000, interval: 5 });
  });

  it('keeps the tab open when the process fails', async () => {
    exit(1);
    await import('./tab-lazy');
    await Promise.resolve();
    expect(allTabs(workspace.layout)).toEqual(['other', `term:${id}`]);
    expect(app.selectedKey).toBe(`term:${id}`);
  });
});
