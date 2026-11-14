import { describe, expect, it } from 'vitest';
import { MOCK_AGENTS, MOCK_PROFILES } from '../daemon/mock';
import { clean, slugId, validateProfile } from './profile-edit';
import type { Profile, ProfilesStore } from '../types';

const store = (over: Partial<ProfilesStore> = {}): ProfilesStore => ({
  ...structuredClone(MOCK_PROFILES),
  profiles: [...structuredClone(MOCK_PROFILES.profiles), { id: 'lean', name: 'Lean', agents: { claude: { model: 'haiku', effort: 'low' } } }],
  default: 'balanced',
  scopes: { '/w/app': 'economy', '/w/app/sub': 'lean', '/w/gone': 'deleted' },
  ...over,
});

describe('profile editor validation', () => {
  const s = store();
  const ok: Profile = { id: 'mine', name: 'Mine', agents: { claude: { model: 'haiku', subagents: { reviewer: { model: 'sonnet', effort: 'low' } } }, codex: { subagentEffort: 'low' } } };

  it('accepts what each CLI supports', () => {
    expect(validateProfile(ok, s.support, MOCK_AGENTS, [])).toEqual([]);
    expect(validateProfile({ ...ok, agents: { agy: { effort: 'xhigh', subagents: { 'security-reviewer': { model: 'pro' } } }, gemini: { subagents: { researcher: { model: 'gemini-2.5-pro' } } } } }, s.support, MOCK_AGENTS, [])).toEqual([]);
  });

  it('rejects unsupported fields, bad values and reserved ids', () => {
    const bad = (p: Partial<Profile>, others: Profile[] = []) => validateProfile({ ...ok, ...p }, s.support, MOCK_AGENTS, others);
    expect(bad({ id: 'max' })).toContain('invalid id max');
    expect(bad({ name: '  ' })).toContain('name must be 1-40 characters');
    expect(bad({}, [ok])).toContain('duplicate id mine');
    expect(bad({ agents: { gemini: { effort: 'low' } } })).toContain('Gemini CLI: effort not supported');
    expect(bad({ agents: { codex: { subagentForce: true } } })).toContain('Codex: subagentForce not supported');
    expect(bad({ agents: { claude: { model: 'opus\nx: y' } } })[0]).toMatch(/invalid model/);
    expect(bad({ agents: { codex: { subagents: { x: { model: 'a' } } } } })).toContain('Codex: subagent overrides not supported');
    expect(bad({ agents: { opencode: { subagents: { x: { effort: 'low' } } } } })).toContain('opencode: invalid effort for x');
    expect(bad({ agents: { agy: { subagents: { reviewer: { model: 'gemini-3-pro' } } } } })).toContain('Antigravity: invalid Antigravity subagent model gemini-3-pro');
    expect(bad({ agents: { agy: { subagents: { reviewer: { model: 'flash_lite' } } } } })).toContain('Antigravity: invalid Antigravity subagent model flash_lite');
    expect(bad({ agents: { nope: {} } })).toContain('nope: unknown agent');
  });

  it('derives ids and drops empty fields', () => {
    expect(slugId('My Preset!', [])).toBe('my-preset');
    expect(slugId('Max', [])).toBe('max-2');
    expect(slugId('x', ['x', 'x-2'])).toBe('x-3');
    expect(slugId('***', [])).toBe('profile');
    expect(clean({ model: '', effort: 'low', subagents: { a: { model: '' }, b: { model: 'haiku' } } })).toEqual({ effort: 'low', subagents: { b: { model: 'haiku' } } });
  });
});
