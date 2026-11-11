import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Task } from '../../types';

const base = { id: 'a', number: 1, title: 't', description: '', project: '/p', status: 'pending', agent: 'claude', model: '', effort: 'high', rank: 1, station: 'atlas', createdAt: '', updatedAt: '' } as Task;

async function html(file: 'QuorumChips' | 'QuorumField', props: Record<string, unknown>, tasks: Task[] = []) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  app.data = { ...app.data, tasks };
  const { render } = await import('svelte/server');
  const mod = file === 'QuorumChips' ? await import('./QuorumChips.svelte') : await import('./QuorumField.svelte');
  return render(mod.default as never, { props } as never).body.split(/<!--[\s\S]*?--!?>/).join('');
}

afterEach(() => vi.resetModules());

describe('QuorumChips', () => {
  it('renders nothing for a plain task', async () => {
    expect(await html('QuorumChips', { task: base })).not.toContain('class="q');
  });

  it('badges the root task with its agents in the title', async () => {
    const out = await html('QuorumChips', { task: { ...base, quorum: { workers: ['claude', 'codex'], reviewers: ['gemini'] } } });
    expect(out).toContain('Quorum');
    expect(out).toContain('workers Claude Code, Codex, reviewers Gemini CLI');
  });

  it('marks a reviewer task with the number of the worker it reviews', async () => {
    const worker = { ...base, id: 'w', number: 7 };
    const out = await html('QuorumChips', { task: { ...base, id: 'r', origin: 'reviewer', parentId: 'w' } }, [worker]);
    expect(out).toContain('Reviewer #7');
    expect(out).toContain('Reviews the result of task #7');
  });

  it('shows the tally, outcome and round on a worker', async () => {
    const votes = { round: 2, voters: 3, needed: 2, outcome: 'accepted', votes: [{ from: 'r', number: 2, agent: 'codex', role: 'reviewer', approve: true, reason: '' }, { from: 'o', number: 1, agent: 'claude', role: 'orchestrator', approve: true, reason: '' }] };
    const out = await html('QuorumChips', { task: { ...base, votes } });
    expect(out).toContain('q accepted');
    expect(out).toContain('2/3 accepted · Round 2');
    expect(out).toContain('2 of 3 votes needed');
  });

  it('shows rejected and voting states', async () => {
    const mk = (outcome: string | null) => ({ ...base, votes: { round: 1, voters: 2, needed: 2, outcome, votes: [] } });
    expect(await html('QuorumChips', { task: mk('rejected') })).toContain('0/2 rejected');
    expect(await html('QuorumChips', { task: mk(null) })).toContain('0/2 voting');
  });
});

describe('QuorumField', () => {
  const agents = [
    { id: 'claude', name: 'Claude Code', installed: true, efforts: [] },
    { id: 'gemini', name: 'Gemini CLI', installed: true, efforts: [] },
    { id: 'amp', name: 'Amp', installed: false, efforts: [] },
  ];
  const q = { workers: ['claude', 'gemini'], reviewers: ['gemini'] };
  const props = { agents, agent: 'claude', on: false, value: q, onon: () => {}, onchange: () => {} };

  it('carries the tour target and the toggle', async () => {
    const out = await html('QuorumField', props);
    expect(out).toContain('data-tour="quorum"');
    expect(out).toContain('type="checkbox"');
    expect(out).not.toContain('Workers');
  });

  it('lists installed agents as worker and reviewer chips once on', async () => {
    const out = await html('QuorumField', { ...props, on: true });
    expect(out).toContain('Workers');
    expect(out).toContain('Reviewers (1 or 2)');
    expect(out.match(/aria-pressed="true"/g)).toHaveLength(3);
    expect(out.match(/aria-pressed="false"/g)).toHaveLength(1);
    expect(out).not.toContain('Amp');
  });

  it('disables the toggle with a hint when the agent cannot orchestrate', async () => {
    const out = await html('QuorumField', { ...props, agent: 'gemini', on: true });
    expect(out).toMatch(/<input[^>]*disabled/);
    expect(out).not.toMatch(/<input[^>]*checked/);
    expect(out).toContain('Only Claude Code and Codex can run a quorum.');
    expect(out).not.toContain('Workers');
  });
});
