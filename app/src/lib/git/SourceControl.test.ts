import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { declOf } from '../../test/css';
import { Scm, commitRequest, showsBranch } from './scm.svelte';
import { gt } from './strings';
import type { ScmFile, ScmStatus } from './types';
import SourceControl from './SourceControl.svelte';
import sourceControl from './SourceControl.svelte?raw';
import graph from './GitGraphTab.svelte?raw';

const file = (path: string, over: Partial<ScmFile> = {}): ScmFile => ({ path, status: 'M', ...over }) as ScmFile;
const status = (over: Partial<ScmStatus> = {}): ScmStatus => ({
  worktree: '/w',
  project: '/p',
  branch: 'feat/x',
  head: 'abc1234',
  upstream: 'origin/feat/x',
  base: 'main',
  ahead: 0,
  behind: 0,
  staged: [],
  changes: [],
  conflicts: [],
  outgoing: [],
  incoming: [],
  stashes: 0,
  branches: ['main', 'feat/x'],
  submodules: [],
  operation: null,
  ...over,
});
const html = (s: ScmStatus | null, props: Record<string, unknown> = {}, scmOver: Partial<Scm> = {}) => {
  const scm = new Scm(() => null);
  scm.status = s;
  Object.assign(scm, scmOver);
  return render(SourceControl, { props: { scm, ...props } }).body;
};

describe('source control states', () => {
  it('shows a loading line until the status arrives', () => {
    expect(html(null)).toContain(gt('loading'));
  });

  it('shows the error with a retry button when the first load failed', () => {
    const h = html(null, {}, { error: 'daemon down' });
    expect(h).toContain('daemon down');
    expect(h).toContain(gt('refresh'));
  });

  it('says the tree is clean when no file changed', () => {
    expect(html(status())).toContain(gt('sc.clean'));
  });
});

describe('source control file lists', () => {
  it('lists staged files and changes under their own counted sections', () => {
    const h = html(status({ staged: [file('src/a.ts', { status: 'A' })], changes: [file('src/b.ts'), file('c.ts', { status: '?' })] }));
    expect(h).toContain(gt('sc.staged'));
    expect(h).toContain(gt('sc.changes'));
    expect(h.match(/<div class="file /g)).toHaveLength(3);
    expect(h).toContain('>b.ts<');
    expect(h).toContain('>src<');
    expect(h).not.toContain(gt('sc.clean'));
  });

  it('shows the conflicts first, outgoing and incoming commits only in the full view', () => {
    const s = status({ conflicts: [file('x.ts', { status: 'U' })], outgoing: [{ sha: '1234567890', subject: 'out one' }], incoming: [{ sha: 'abcdef0123', subject: 'in one' }] });
    const full = html(s);
    expect(full.indexOf(gt('sc.conflicts'))).toBeGreaterThan(-1);
    expect(full).toContain('out one');
    expect(full).toContain('in one');
    const only = html(s, { filesOnly: true });
    expect(only).toContain(gt('sc.conflicts'));
    expect(only).not.toContain('out one');
    expect(only).not.toContain('in one');
  });

  it('offers a discard button per changed file and shows no confirmation before it is pressed', () => {
    const h = html(status({ changes: [file('a.ts')] }));
    expect(h).toContain(`aria-label="${gt('sc.discardFile', { file: 'a.ts' })}"`);
    expect(h).not.toContain('role="alertdialog"');
  });
});

describe('source control commit box', () => {
  it('labels the commit button by what it will commit', () => {
    expect(html(status())).toContain(`${gt('sc.commit')}</button>`);
    expect(html(status({ changes: [file('a'), file('b')] }))).toContain(gt('sc.commitAll', { n: 2 }));
    expect(html(status({ staged: [file('a')], changes: [file('b')] }))).toContain(gt('sc.commitN', { n: 1 }));
  });

  it('keeps the commit button off until there is a message', () => {
    expect(html(status({ changes: [file('a')] }))).toMatch(/<button class="btn primary main[^"]*"[^>]*disabled/);
  });

  it('leaves the commit box and the sync buttons to the full view', () => {
    const s = status({ changes: [file('a')] });
    expect(html(s)).toContain('<textarea');
    expect(html(s)).toContain(gt('sc.fetch'));
    const only = html(s, { filesOnly: true });
    expect(only).not.toContain('<textarea');
    expect(only).not.toContain(gt('sc.fetch'));
  });

  it('announces a merge in progress', () => {
    expect(html(status({ operation: 'merge' }))).toContain(gt('sc.op.merge'));
    expect(html(status())).not.toContain('class="op"');
  });
});

describe('submodules in source control', () => {
  const sub = (over: NonNullable<ScmFile['submodule']>) => status({ changes: [file('libs/x', { submodule: over })], submodules: [{ path: 'libs/x', worktree: '/w/libs/x' }] });

  it('stages a submodule pointer only when its commit moved', () => {
    expect(html(sub({ commit: true, modified: false, untracked: false }))).toContain(gt('sc.stagePointer'));
    expect(html(sub({ commit: false, modified: true, untracked: false }))).not.toContain(gt('sc.stagePointer'));
  });

  it('opens a submodule with edits inside, only when the caller can open worktrees', () => {
    const s = sub({ commit: false, modified: true, untracked: false });
    expect(html(s, { onopen: () => {} })).toContain(gt('sc.openSub', { name: 'x' }));
    expect(html(s)).not.toContain(gt('sc.openSub', { name: 'x' }));
    expect(html(sub({ commit: true, modified: false, untracked: false }), { onopen: () => {} })).not.toContain(gt('sc.openSub', { name: 'x' }));
  });

  it('gives a submodule no per-file discard', () => {
    expect(html(sub({ commit: true, modified: false, untracked: false }))).not.toContain(gt('sc.discardFile', { file: 'libs/x' }));
  });
});

describe('commit request', () => {
  it('commits everything when nothing is staged, and only the index when something is', () => {
    expect(commitRequest(status({ changes: [file('a')] }), '  msg  ').all).toBe(true);
    expect(commitRequest(status({ staged: [file('a')] }), 'msg').all).toBeUndefined();
  });

  it('forces all on request, trims the message and forwards amend and push', () => {
    expect(commitRequest(status({ staged: [file('a')] }), ' m \n', { all: true, amend: true, push: true })).toEqual({ message: 'm', amend: true, push: true, all: true });
  });
});

describe('branch label', () => {
  it('shows the branch unless the worktree folder name already says it', () => {
    expect(showsBranch('feat/login', 'feat-login')).toBe(false);
    expect(showsBranch('feat/login', 'login')).toBe(true);
    expect(showsBranch(null, 'anything')).toBe(true);
  });
});

describe('CSS guard: git tab on a phone', () => {
  it('scrolls the changes as one page', () => {
    expect(declOf(graph, '.scm.full', 'overflow-y')).toBe('auto');
  });

  it('hides the author and sha columns and wraps the toolbar', () => {
    expect(declOf(graph, '.graph.phone .c-author', 'display')).toBe('none');
    expect(declOf(graph, '.graph.phone .c-sha', 'display')).toBe('none');
    expect(declOf(graph, '.graph.phone .toolbar', 'flex-wrap')).toBe('wrap');
  });
});

describe('CSS guard: source control on touch screens', () => {
  it('shows the file actions where there is no hover', () => {
    expect(declOf(sourceControl, '.acts', 'opacity', '@media (hover: none)')).toBe('1');
  });
});

describe('git entry points in the palette', () => {
  it('leaves the review modes to the palette', async () => {
    const { command } = await import('../commands/commands');
    const { registerMore } = await import('../commands/commands-more');
    registerMore();
    expect(command('review-beside').label!()).toBe('Review beside the agent');
    expect(command('review-close').section).toBe('actions');
    expect(command('live-review').label!()).toBe('Open or close the live review');
    expect(command('graph').section).toBe('views');
  });

  it('opens the Git tab on phone from More', async () => {
    const { app } = await import('../state.svelte');
    const open = vi.spyOn(app, 'openProjectTab').mockImplementation(() => {});
    const { command } = await import('../commands/commands');
    const { registerMore } = await import('../commands/commands-more');
    registerMore();
    command('graph').run();
    expect(open).toHaveBeenCalledWith('graph');
    open.mockRestore();
  });
});
