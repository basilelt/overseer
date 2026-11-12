import { findAgent, pickAgent } from '../agents';
import { app } from '../state.svelte';
import { settings } from '../settings.svelte';
import { newTabKey } from '../tabs';
import { reviewKey } from '../review/keys';
import { saveBase } from '../review/base';
import { ui, isPhone } from '../mobile/ui.svelte';
import { focusPane, openInFocused, split, update, type Layout } from '../workspace.svelte';
import { gt } from './strings';
import { githubApi } from './api';
import type { ReviewRequest, StartedReview } from './types';
import type { Term } from '../types';
import { hostKey } from '../window';

export const PR_REVIEWS_KEY = newTabKey('prs', 'github');

export function openPrReviews() {
  app.open(null);
  app.view = 'agents';
  if (isPhone()) ui.tab = 'agents';
  update((l) => openInFocused(l, PR_REVIEWS_KEY));
  app.selectedKey = PR_REVIEWS_KEY;
}

export function beside(l: Layout, review: string, term: string, phone: boolean): Layout {
  const withReview = openInFocused(l, review);
  if (phone) return openInFocused(withReview, term);
  const next = withReview.panes.length < 2 ? split(withReview, 'horizontal') : focusPane(withReview, withReview.focused === 0 ? 1 : 0);
  return openInFocused(next, term);
}

function show(term: Term, worktree: string, base: string) {
  saveBase(worktree, `origin/${base}`);
  if (!app.data.terms.some((t) => t.id === term.id)) app.data = { ...app.data, terms: [...app.data.terms, term] };
  const key = `term:${term.id}`;
  app.view = 'agents';
  update((l) => beside(l, reviewKey({ worktree }), key, isPhone()));
  app.selectedKey = key;
}

export function newTerm(terms: Term[], title: string, known: Set<string>): Term | undefined {
  return terms.find((x) => x.title === title && !known.has(x.id));
}

export function waitStep(from: string, host: string, terms: Term[], title: string, known: Set<string>): Term | 'abort' | null {
  if (host !== from) return 'abort';
  return newTerm(terms, title, known) ?? null;
}

export function whenTerm(title: string, known: Set<string>, then: (t: Term) => void, limitMs = 15 * 60_000) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let done = false;
  const from = hostKey(app.conn.target);
  const stop = $effect.root(() => {
    $effect(() => {
      const step = waitStep(from, hostKey(app.conn.target), app.data.terms, title, known);
      if (!step || done) return;
      done = true;
      queueMicrotask(() => {
        stop();
        clearTimeout(timer);
        if (step !== 'abort') then(step);
      });
    });
  });
  timer = setTimeout(stop, limitMs);
  return stop;
}

export async function startReview(pr: ReviewRequest): Promise<StartedReview | null> {
  const client = app.client;
  if (!client) throw new Error('not connected');
  const agentId = pickAgent(app.agents, settings.defaultAgent);
  const agent = findAgent(app.agents, agentId);
  const same = agentId === settings.defaultAgent;
  const [owner, name] = pr.repo.split('/');
  const known = new Set(app.data.terms.map((t) => t.id));
  const res = await githubApi(client).startReview(owner, name, pr.number, {
    agent: agentId,
    model: same ? settings.defaultModel || undefined : undefined,
    effort: same && agent?.efforts?.includes(settings.defaultEffort) ? settings.defaultEffort : undefined,
    cols: 120,
    rows: 32,
  });
  const started = gt('started', { agent: agent?.name ?? agentId, n: pr.number });
  if ('pending' in res) {
    app.notify(gt('preparing', { n: pr.number }));
    whenTerm(res.title, known, (t) => {
      show(t, t.cwd, pr.baseRefName);
      app.notify(started);
    });
    return null;
  }
  show(res.term, res.worktree, pr.baseRefName);
  app.notify(started);
  return res;
}
