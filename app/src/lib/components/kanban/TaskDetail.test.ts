import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import { t } from '../../i18n.svelte';
import { FALLBACK_AGENTS } from '../../agents';
import type { Profile, ProfilesStore, Task } from '../../types';
import TaskDetail from './TaskDetail.svelte';
import source from './TaskDetail.svelte?raw';
import { parseStyle } from '../../../test/css';

const task = (over: Partial<Task> = {}) =>
  ({
    id: 't1',
    number: 198,
    title: 'Bug: cli not interactable',
    description: '**Type:** Bug\nI cannot select text',
    status: 'pending',
    project: '/repo',
    agent: 'claude',
    model: 'opus',
    effort: 'medium',
    rank: 1,
    station: '',
    ticketKey: 'PROJ-1',
    taskType: 'fix',
    profile: 'balanced',
    createdAt: '2026-11-19T08:00:00Z',
    updatedAt: '2026-11-19T08:00:00Z',
    ...over,
  }) as Task;

const builtin = { id: 'balanced', name: 'Balanced', builtin: true, agents: {} } as Profile;
const store = (over: Partial<ProfilesStore> = {}): ProfilesStore => ({ default: 'balanced', profiles: [builtin], scopes: {}, support: {}, ...over });

const before = { data: app.data, overlayArg: app.overlayArg, agents: app.agents, profiles: app.profiles, projects: app.projects };
afterEach(() => Object.assign(app, before));

const html = (over: Partial<Task> = {}, props: Record<string, unknown> = {}, profiles: ProfilesStore | null = store({ taskTypes: ['fix', 'feature'] })) => {
  app.data = { ...app.data, tasks: [task(over)] };
  app.overlayArg = 't1';
  app.agents = FALLBACK_AGENTS;
  app.profiles = profiles;
  app.projects = [{ path: '/repo', name: 'repo' }];
  return render(TaskDetail, { props: { onclose: () => {}, ...props } }).body;
};

describe('TaskDetail view mode', () => {
  it('shows the task text and an edit button pointed at by the tour', () => {
    const h = html();
    expect(h).toContain('Bug: cli not interactable');
    expect(h).toContain('**Type:** Bug');
    expect(h).toContain('data-tour="task-edit"');
    expect(h).toContain(t('task.edit'));
  });

  it('does not show the edit form before the button is used', () => {
    const h = html();
    expect(h).not.toContain('class="edit');
    expect(h).not.toContain(t('common.save'));
  });
});

describe('TaskDetail edit mode', () => {
  it('prefills title, description, ticket, type and profile, and offers the agent and model', () => {
    const h = html({}, { startEditing: true });
    expect(h).toContain('class="edit');
    expect(h).toMatch(/<input[^>]*value="Bug: cli not interactable"/);
    expect(h).toContain('**Type:** Bug');
    expect(h).toMatch(/<input[^>]*value="PROJ-1"/);
    expect(h).toContain('<option value="fix"');
    expect(h).toContain('<option value="balanced"');
    expect(h).toContain(`${t('task.agent')}: Claude`);
    expect(h).toContain('opus');
    expect(h).toContain(t('common.save'));
    expect(h).toContain(t('common.cancel'));
  });

  it('offers the installed task agents in the picker, not the shell', () => {
    const h = html({}, { startEditing: true });
    const picker = h.slice(h.indexOf('class="picker'), h.indexOf('</button>', h.indexOf('class="picker')));
    expect(picker).not.toContain('Shell');
    expect(picker).toContain('Claude');
    expect(h).toContain(`${t('task.agent')}: Claude`);
  });

  it('keeps a session task to its text: no agent, model or effort fields', () => {
    const h = html({ origin: 'session' }, { startEditing: true });
    expect(h).toContain('class="edit');
    expect(h).not.toContain('class="picker');
    expect(h).not.toContain(t('task.effort'));
    expect(h).toMatch(/<input[^>]*value="Bug: cli not interactable"/);
  });

  it('hides the type and profile fields when the daemon has no profiles', () => {
    const h = html({}, { startEditing: true }, null);
    expect(h).not.toContain('<option value="fix"');
    expect(h).not.toContain(t('profile.label'));
    expect(h).toContain(`${t('task.agent')}: Claude`);
  });
});

describe('TaskDetail text selection', () => {
  it('lets the user select and copy the title, description, meta, messages and votes, which the body turns off', () => {
    const wanted = ['.title', '.desc', '.meta', '.msg', '.votes li'];
    const rules = parseStyle(source);
    for (const sel of wanted) {
      const hit = rules.filter((r) => r.selector === sel).some((r) => r.decls['user-select'] === 'text' && r.decls['-webkit-user-select'] === 'text');
      expect(hit, sel).toBe(true);
    }
  });
});
