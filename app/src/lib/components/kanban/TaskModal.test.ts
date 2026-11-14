import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { t } from '../../i18n.svelte';
import { app } from '../../state.svelte';
import type { Profile, ProfilesStore } from '../../types';
import TaskModal from './TaskModal.svelte';

const builtin = { id: 'balanced', name: 'Balanced', builtin: true, agents: {} } as Profile;
const store = (over: Partial<ProfilesStore> = {}): ProfilesStore => ({ default: 'balanced', profiles: [builtin], scopes: {}, support: {}, ...over });
const before = { profiles: app.profiles };
afterEach(() => Object.assign(app, before));

const html = (profiles: ProfilesStore | null) => {
  app.profiles = profiles;
  return render(TaskModal, { props: { onclose: () => {} } }).body;
};

describe('task type in the add task form', () => {
  it('offers the task type with every type listed when the daemon knows task types', () => {
    const h = html(store({ taskTypes: ['fix', 'feature'] }));
    expect(h).toContain('data-tour="task-type"');
    expect(h).toContain(`>${t('taskType.unset')}<`);
    for (const ty of ['fix', 'feature', 'engineering', 'refactor', 'triage'] as const) expect(h).toContain(`<option value="${ty}"`);
  });

  it('keeps only the profile field on a daemon without task types', () => {
    const h = html(store());
    expect(h).not.toContain('data-tour="task-type"');
    expect(h).toContain(t('profile.label'));
  });

  it('shows neither field when the daemon has no profiles', () => {
    const h = html(null);
    expect(h).not.toContain('data-tour="task-type"');
    expect(h).not.toContain(t('profile.label'));
  });
});
