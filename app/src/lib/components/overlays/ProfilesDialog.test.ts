import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { declOf } from '../../../test/css';
import { app } from '../../state.svelte';
import type { AgentInfo, Profile, ProfilesStore } from '../../types';
import ProfilesDialog from './ProfilesDialog.svelte';
import source from './ProfilesDialog.svelte?raw';

const agent = (id: string): AgentInfo => ({ id, name: id.toUpperCase(), installed: true, efforts: [], resume: false, verified: true, sessions: true, turns: true });
const builtin = { id: 'balanced', name: 'Balanced', builtin: true, agents: {} } as Profile;
const store = (over: Partial<ProfilesStore> = {}): ProfilesStore => ({ default: 'balanced', profiles: [builtin], scopes: {}, support: { claude: { model: 'flag' }, codex: { model: 'flag' } }, ...over });
const before = { profiles: app.profiles, agents: app.agents };
afterEach(() => Object.assign(app, before));

const html = (s: ProfilesStore, agents = ['claude']) => {
  app.profiles = s;
  app.agents = agents.map(agent);
  return render(ProfilesDialog, { props: { onclose: () => {} } }).body;
};
const details = (h: string, cls: string) => [...h.matchAll(new RegExp(`<details[^>]*class="${cls}[^"]*"[^>]*>`, 'g'))].map((m) => m[0]);

describe('agent sections of the profiles dialog', () => {
  it('opens the only agent section by default', () => {
    const sections = details(html(store()), 'agent');
    expect(sections).toHaveLength(1);
    expect(sections[0]).toMatch(/\bopen\b/);
  });

  it('keeps the agent sections folded when there are several', () => {
    const sections = details(html(store(), ['claude', 'codex']), 'agent');
    expect(sections).toHaveLength(2);
    for (const s of sections) expect(s).not.toMatch(/\bopen\b/);
  });
});

describe('task type rules of the profiles dialog', () => {
  it('hides the task type rules on a daemon that has no task types', () => {
    expect(html(store())).not.toContain('data-tour="profile-types"');
  });

  it('shows one select per task type, with the mapped profile chosen', () => {
    const h = html(store({ taskTypes: ['fix', 'feature'], types: { fix: 'balanced' } }));
    expect(h).toContain('data-tour="profile-types"');
    expect(h).toMatch(/<select aria-label="Bug fix"[^>]*>/);
    expect(h.match(/<select aria-label="Bug fix".*?<\/select>/s)?.[0]).toMatch(/<option value="balanced"[^>]*selected/);
  });

  it('opens the task type section only once a rule exists', () => {
    expect(details(html(store({ taskTypes: ['fix'] })), 'types')[0]).not.toMatch(/\bopen\b/);
    expect(details(html(store({ taskTypes: ['fix'], types: { fix: 'balanced' } })), 'types')[0]).toMatch(/\bopen\b/);
  });
});

describe('CSS guard: profiles dialog on a phone', () => {
  it('wraps the profile actions so Delete stays visible', () => {
    expect(declOf(source, ':global(.phone) .head', 'flex-wrap')).toBe('wrap');
    expect(declOf(source, ':global(.phone) .head .name', 'width')).toBe('100%');
    expect(declOf(source, ':global(.phone) .head .spacer', 'display')).toBe('none');
  });
});
