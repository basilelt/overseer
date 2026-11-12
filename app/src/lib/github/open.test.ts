import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
import type { AgentInfo, Term } from '../types';
import { app } from '../state.svelte';
import { settings } from '../settings/settings.svelte';
import { ui } from '../mobile/ui.svelte';
import { loadBase } from '../review/base';
import { reviewKey } from '../review/keys';
import { layoutFromTabs, split, workspace, type Layout } from '../workspace/workspace.svelte';
import { beside, newTerm, openPrReviews, PR_REVIEWS_KEY, startReview, waitStep } from './open.svelte';
import type { ReviewRequest } from './types';

const term = (id: string, title: string) => ({ id, title, cwd: '/w' }) as Term;

describe('waiting for a PR review terminal', () => {
  const known = new Set(['old']);
  const terms = [term('old', 'PR #1')];

  it('waits until a new terminal with the title shows up', () => {
    expect(waitStep('ssh:a', 'ssh:a', terms, 'PR #1', known)).toBeNull();
    expect(waitStep('ssh:a', 'ssh:a', [...terms, term('x', 'PR #2')], 'PR #1', known)).toBeNull();
    expect(waitStep('ssh:a', 'ssh:a', [...terms, term('new', 'PR #1')], 'PR #1', known)).toMatchObject({ id: 'new' });
  });

  it('gives up once the window switched host, even if a matching terminal appears there', () => {
    expect(waitStep('ssh:a', 'ssh:b', [...terms, term('new', 'PR #1')], 'PR #1', known)).toBe('abort');
    expect(waitStep('ssh:a', 'local', terms, 'PR #1', known)).toBe('abort');
  });

  it('picks the first unknown terminal with the title', () => {
    expect(newTerm([term('old', 'PR #1'), term('a', 'PR #1'), term('b', 'PR #1')], 'PR #1', known)?.id).toBe('a');
    expect(newTerm([], 'PR #1', known)).toBeUndefined();
  });
});

describe('beside', () => {
  const review = 'review:wt:/w';
  const t = 'term:r1';

  it('stacks the review and its terminal in one pane on phones', () => {
    const l = beside(layoutFromTabs(['line:a']), review, t, true);
    expect(l.panes).toHaveLength(1);
    expect(l.panes[0]).toEqual({ tabs: ['line:a', review, t], active: t });
  });

  it('splits a single pane and puts the terminal next to the review', () => {
    const l = beside(layoutFromTabs(['line:a']), review, t, false);
    expect(l.split).toBe('horizontal');
    expect(l.panes.map((p) => p.tabs)).toEqual([['line:a', review], [t]]);
    expect(l.panes[0].active).toBe(review);
    expect(l.focused).toBe(1);
  });

  it('uses the other existing pane instead of splitting again', () => {
    const two: Layout = { ...split(layoutFromTabs(['line:a']), 'vertical'), focused: 1 };
    const l = beside(two, review, t, false);
    expect(l.split).toBe('vertical');
    expect(l.panes.map((p) => p.tabs)).toEqual([['line:a', t], [review]]);
    expect(l.focused).toBe(0);
  });

  it('reuses the pane that already shows the review', () => {
    const l0 = beside(layoutFromTabs(['line:a']), review, t, false);
    const l1 = beside({ ...l0, focused: 1 }, review, 'term:r2', false);
    expect(l1.panes.map((p) => p.tabs)).toEqual([['line:a', review], [t, 'term:r2']]);
  });
});

describe('opening PR reviews', () => {
  const pr: ReviewRequest = { repo: 'acme/web app', number: 42, title: 'Fix', url: 'https://x', author: null, headRefName: 'fix', baseRefName: 'develop', updatedAt: '' } as ReviewRequest;
  const agents = [
    { id: 'claude', name: 'Claude Code', installed: true, efforts: ['low', 'high'] },
    { id: 'codex', name: 'Codex', installed: true },
  ] as unknown as AgentInfo[];
  const saved = { layout: workspace.layout, data: app.data, client: app.client, agents: app.agents, selectedKey: app.selectedKey, view: app.view };
  const savedSettings = { defaultAgent: settings.defaultAgent, defaultModel: settings.defaultModel, defaultEffort: settings.defaultEffort };
  let request: ReturnType<typeof vi.fn>;
  let notify: MockInstance<typeof app.notify>;
  let kv: Map<string, string>;

  beforeEach(() => {
    kv = new Map();
    vi.stubGlobal('localStorage', { getItem: (k: string) => kv.get(k) ?? null, setItem: (k: string, v: string) => void kv.set(k, v) });
    request = vi.fn();
    notify = vi.spyOn(app, 'notify').mockImplementation(() => {});
    workspace.layout = layoutFromTabs(['line:a']);
    app.data = { lines: [], terms: [term('old', 'other')], tasks: [], activity: {} };
    app.client = { request } as never;
    app.agents = agents;
    app.selectedKey = 'line:a';
    app.view = 'kanban';
    ui.layout = 'desktop';
    Object.assign(settings, { defaultAgent: 'claude', defaultModel: 'opus', defaultEffort: 'high' });
  });

  afterEach(() => {
    Object.assign(workspace, { layout: saved.layout });
    Object.assign(app, saved);
    Object.assign(settings, savedSettings);
    ui.layout = 'desktop';
    ui.tab = 'agents';
  });

  it('opens the PR reviews tab in the focused pane and selects it', () => {
    vi.spyOn(app, 'open').mockImplementation(() => {});
    ui.layout = 'phone';
    ui.tab = 'more';
    openPrReviews();
    expect(app.open).toHaveBeenCalledWith(null);
    expect(app.view).toBe('agents');
    expect(ui.tab).toBe('agents');
    expect(app.selectedKey).toBe(PR_REVIEWS_KEY);
    expect(workspace.layout.panes[0]).toEqual({ tabs: ['line:a', PR_REVIEWS_KEY], active: PR_REVIEWS_KEY });
  });

  it('refuses to start without a connection', async () => {
    app.client = null;
    await expect(startReview(pr)).rejects.toThrow('not connected');
  });

  it('starts the default agent with its model and effort and shows the review beside its terminal', async () => {
    const t = term('r1', 'PR #42');
    request.mockResolvedValue({ term: t, worktree: '/wt/42', project: '/p', branch: 'fix' });
    const res = await startReview(pr);
    expect(request).toHaveBeenCalledWith('POST', '/v1/github/review-requests/acme/web%20app/42/review', { agent: 'claude', model: 'opus', effort: 'high', cols: 120, rows: 32 });
    expect(res?.term).toBe(t);
    expect(app.data.terms.map((x) => x.id)).toEqual(['old', 'r1']);
    expect(app.selectedKey).toBe('term:r1');
    expect(app.view).toBe('agents');
    expect(workspace.layout.panes.map((p) => p.tabs)).toEqual([['line:a', reviewKey({ worktree: '/wt/42' })], ['term:r1']]);
    expect(loadBase('/wt/42')).toBe('origin/develop');
    expect(notify).toHaveBeenCalledWith('Claude Code is reviewing #42. It will ask before posting anything.');
  });

  it('does not add the terminal twice when the daemon already reported it', async () => {
    const t = term('r1', 'PR #42');
    app.data = { ...app.data, terms: [...app.data.terms, t] };
    request.mockResolvedValue({ term: t, worktree: '/wt/42', project: '/p', branch: 'fix' });
    await startReview(pr);
    expect(app.data.terms.filter((x) => x.id === 'r1')).toHaveLength(1);
  });

  it('leaves model and effort to the agent when it is not the default one', async () => {
    settings.defaultAgent = 'codex';
    app.agents = [agents[0]];
    request.mockResolvedValue({ term: term('r2', 'PR #42'), worktree: '/wt/42', project: '/p', branch: 'fix' });
    await startReview(pr);
    expect(request.mock.calls[0][2]).toMatchObject({ agent: 'claude', model: undefined, effort: undefined });
  });

  it('drops an effort the default agent does not support', async () => {
    settings.defaultEffort = 'max' as never;
    settings.defaultModel = '';
    request.mockResolvedValue({ term: term('r3', 'PR #42'), worktree: '/wt/42', project: '/p', branch: 'fix' });
    await startReview(pr);
    expect(request.mock.calls[0][2]).toMatchObject({ agent: 'claude', model: undefined, effort: undefined });
  });

  it('tells the user it is preparing when the clone is still running', async () => {
    request.mockResolvedValue({ pending: true, title: 'PR #42' });
    expect(await startReview(pr)).toBeNull();
    expect(notify).toHaveBeenCalledWith('Preparing #42 (cloning or fetching). The agent opens when it is ready.');
    expect(app.selectedKey).toBe('line:a');
  });
});
