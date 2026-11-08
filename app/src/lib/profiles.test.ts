import { describe, expect, it } from 'vitest';
import { MOCK_AGENTS, MOCK_PROFILES, MockDaemon } from './mock';
import { chipLabel, chipShort, clean, costIndex, effectiveId, formatIndex, initialChoice, priceOf, resolveProfile, scoped, slugId, validateProfile } from './profiles';
import type { Profile, ProfilesStore, Term } from './types';

const store = (over: Partial<ProfilesStore> = {}): ProfilesStore => ({
  ...structuredClone(MOCK_PROFILES),
  profiles: [...structuredClone(MOCK_PROFILES.profiles), { id: 'lean', name: 'Lean', agents: { claude: { model: 'haiku', effort: 'low' } } }],
  default: 'balanced',
  scopes: { '/w/app': 'economy', '/w/app/sub': 'lean', '/w/gone': 'deleted' },
  ...over,
});
const agent = (id: string) => MOCK_AGENTS.find((a) => a.id === id)!;

describe('profile resolution', () => {
  it('goes task > workspace (longest prefix) > global', () => {
    const s = store();
    expect(effectiveId(s, { cwd: '/elsewhere' })).toBe('balanced');
    expect(effectiveId(s, { cwd: '/w/app' })).toBe('economy');
    expect(effectiveId(s, { cwd: '/w/app/x' })).toBe('economy');
    expect(effectiveId(s, { cwd: '/w/app/sub/deep' })).toBe('lean');
    expect(effectiveId(s, { cwd: '/w/application' })).toBe('balanced');
    expect(effectiveId(s, { explicit: 'max', cwd: '/w/app' })).toBe('max');
    expect(effectiveId(s, { explicit: 'none', cwd: '/w/app' })).toBe('');
    expect(effectiveId(store({ default: '' }), { cwd: '/x' })).toBe('');
    expect(scoped(s, undefined)).toBeUndefined();
  });

  it('ignores stale ids and falls back to the scope', () => {
    const s = store();
    expect(resolveProfile(s, { cwd: '/w/gone' })).toBeUndefined();
    expect(resolveProfile(s, { explicit: 'ghost', cwd: '/w/app' })?.id).toBe('economy');
    expect(resolveProfile(null, { cwd: '/w/app' })).toBeUndefined();
    expect(resolveProfile(s, { explicit: 'none', cwd: '/w/app' })).toBeUndefined();
  });

  it('prefills the task form from the profile, else from the defaults', () => {
    const s = store();
    expect(initialChoice(resolveProfile(s, { cwd: '/w/app' }), agent('claude'), { model: 'opus', effort: 'high' })).toEqual({ model: 'sonnet', effort: 'low' });
    expect(initialChoice(undefined, agent('claude'), { model: 'opus', effort: 'max' })).toEqual({ model: 'opus', effort: 'max' });
    expect(initialChoice(resolveProfile(s, { explicit: 'max' }), agent('gemini'), { model: '', effort: 'high' })).toEqual({ model: '' });
    expect(initialChoice(resolveProfile(s, { explicit: 'max' }), agent('codex'), { model: '', effort: 'low' })).toEqual({ model: 'gpt-5.5', effort: 'max' });
  });
});

describe('labels and cost', () => {
  it('formats the chip', () => {
    expect(chipLabel({ name: 'Balanced', model: 'sonnet', effort: 'medium' })).toBe('Balanced · sonnet · effort medium');
    expect(chipLabel({ model: 'opus' })).toBe('opus');
    expect(chipLabel(undefined)).toBe('');
    expect(chipShort({ model: 'opus' })).toBe('opus');
    expect(chipShort({ name: 'Max', model: 'opus' })).toBe('Max');
  });

  it('prices aliases, full ids and provider prefixes', () => {
    expect(priceOf('sonnet')).toEqual([2, 10]);
    expect(priceOf('claude-opus-4-7')).toEqual([4, 20]);
    expect(priceOf('anthropic/claude-haiku-4-5')).toEqual([0.1, 0.5]);
    expect(priceOf('gpt-5.4-mini-2026')).toEqual([0.75, 4.5]);
    expect(priceOf('mystery')).toBeUndefined();
  });

  it('orders presets economy < balanced < max relative to CLI defaults', () => {
    const s = store();
    const sup = s.support.claude;
    const idx = (id: string) => costIndex(resolveProfile(s, { explicit: id }), 'claude', sup)!;
    expect(idx('economy')).toBeLessThan(idx('balanced'));
    expect(idx('balanced')).toBeLessThan(1);
    expect(idx('max')).toBeGreaterThan(1);
    expect(costIndex(undefined, 'claude', sup)).toBe(1);
    expect(costIndex(undefined, 'amp', s.support.amp)).toBeUndefined();
    const cx = (id: string) => costIndex(resolveProfile(s, { explicit: id }), 'codex', s.support.codex)!;
    expect(cx('economy')).toBeLessThan(cx('balanced'));
    expect(formatIndex(0.25)).toBe('≈0.25×');
    expect(formatIndex(1.6)).toBe('≈1.6×');
    expect(formatIndex(2)).toBe('≈2×');
  });
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

describe('mock daemon applies profiles like the daemon', () => {
  it('uses the workspace scope for new sessions and an explicit profile over it', async () => {
    const d = new MockDaemon();
    await d.request('PUT', '/v1/profiles', { scopes: { '/w/app': 'economy' } });
    const t = await d.request<Term>('POST', '/v1/terms', { agent: 'claude', cwd: '/w/app' });
    expect(t.cmd).toEqual(['claude', '--permission-mode', 'auto', '--model', 'sonnet', '--effort', 'low']);
    expect(t.profile?.name).toBe('Economy');
    const m = await d.request<Term>('POST', '/v1/terms', { agent: 'claude', cwd: '/w/app', profile: 'max' });
    expect(m.cmd).toEqual(['claude', '--permission-mode', 'auto', '--model', 'opus', '--effort', 'max']);
    const plain = await d.request<Term>('POST', '/v1/terms', { agent: 'claude', cwd: '/elsewhere' });
    expect(plain.cmd).toEqual(['claude', '--permission-mode', 'auto']);
    await expect(d.request('POST', '/v1/terms', { agent: 'claude', profile: 'ghost' })).rejects.toThrow(/unknown profile/);
  });

  it('previews then applies subagent file edits per confirmed file', async () => {
    const d = new MockDaemon();
    const mine: Profile = { id: 'mine', name: 'Mine', agents: { claude: { subagents: { 'debug-specialist': { model: 'haiku' }, 'test-writer': { model: 'sonnet', effort: 'high' } } } } };
    await d.request('PUT', '/v1/profiles', { profiles: [mine] });
    const dry = await d.request<{ files: { path: string; changes: unknown[] }[] }>('POST', '/v1/profiles/apply-files', { profile: 'mine', dryRun: true });
    expect(dry.files.map((f) => f.changes)).toEqual([[{ key: 'model', from: 'opus', to: 'haiku' }]]);
    const run = await d.request<{ files: { written: boolean; backup: string }[] }>('POST', '/v1/profiles/apply-files', { profile: 'mine', dryRun: false, files: [] });
    expect(run.files).toEqual([]);
    const yes = await d.request<{ files: { written: boolean; backup: string }[] }>('POST', '/v1/profiles/apply-files', { profile: 'mine', dryRun: false, files: [dry.files[0].path] });
    expect(yes.files[0].written).toBe(true);
    expect(yes.files[0].backup).toMatch(/\.bak-\d+$/);
  });

  it('previews Gemini and Antigravity named subagent model edits', async () => {
    const d = new MockDaemon();
    const mine: Profile = {
      id: 'google-agents',
      name: 'Google agents',
      agents: {
        gemini: { subagents: { researcher: { model: 'gemini-2.5-pro' } } },
        agy: { subagents: { 'security-reviewer': { model: 'flash' } } },
      },
    };
    await d.request('PUT', '/v1/profiles', { profiles: [mine] });
    const dry = await d.request<{ files: { path: string; changes: { key: string; to: string }[] }[] }>('POST', '/v1/profiles/apply-files', { profile: mine.id, dryRun: true });
    expect(dry.files.map((f) => f.changes)).toEqual([
      [{ key: 'model', from: 'pro', to: 'flash' }],
      [{ key: 'model', from: 'gemini-2.5-flash', to: 'gemini-2.5-pro' }],
    ]);
    const yes = await d.request<{ files: { path: string; written: boolean }[] }>('POST', '/v1/profiles/apply-files', { profile: mine.id, dryRun: false, files: dry.files.map((f) => f.path) });
    expect(yes.files.map((f) => f.written)).toEqual([true, true]);
  });
});
