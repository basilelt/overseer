import { describe, expect, it } from 'vitest';
import { MOCK_AGENTS, MOCK_PROFILES, MockDaemon } from './mock';
import { chipLabel, chipShort, costIndex, effectiveId, formatIndex, initialChoice, priceOf, resolveProfile, scoped, TASK_TYPES, typed } from './profiles';
import type { Profile, ProfilesStore, Task, Term } from './types';

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

  it('puts the task type between the explicit profile and the workspace', () => {
    const s = store({ types: { fix: 'lean', triage: 'none', refactor: 'deleted' } });
    expect(effectiveId(s, { taskType: 'fix', cwd: '/w/app' })).toBe('lean');
    expect(effectiveId(s, { taskType: 'fix', cwd: '/elsewhere' })).toBe('lean');
    expect(effectiveId(s, { explicit: 'max', taskType: 'fix', cwd: '/w/app' })).toBe('max');
    expect(effectiveId(s, { taskType: 'triage', cwd: '/w/app' })).toBe('');
    expect(effectiveId(s, { taskType: 'refactor', cwd: '/w/app' })).toBe('economy');
    expect(effectiveId(s, { taskType: 'feature', cwd: '/w/app' })).toBe('economy');
    expect(effectiveId(s, { taskType: null, cwd: '/w/app' })).toBe('economy');
    expect(typed(store(), 'fix')).toBeUndefined();
  });

  it('keeps the task type when an explicit profile is stale', () => {
    const s = store({ types: { fix: 'lean' } });
    expect(resolveProfile(s, { explicit: 'ghost', taskType: 'fix', cwd: '/w/app' })?.id).toBe('lean');
    expect(resolveProfile(s, { taskType: 'triage', cwd: '/w/app' })?.id).toBe('economy');
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

  it('validates task type rules with the daemon errors', async () => {
    const d = new MockDaemon();
    expect(TASK_TYPES).toEqual(['fix', 'feature', 'engineering', 'refactor', 'triage']);
    await expect(d.request('PUT', '/v1/profiles', { types: { migration: 'max' } })).rejects.toThrow('unknown task type "migration", expected one of fix, feature, engineering, refactor, triage');
    await expect(d.request('PUT', '/v1/profiles', { types: { fix: 'ghost' } })).rejects.toThrow('task type fix: unknown profile ghost');
    await expect(d.request('PUT', '/v1/profiles', { types: { fix: '' } })).rejects.toThrow('task type fix: unknown profile');
    const s = await d.request<ProfilesStore>('PUT', '/v1/profiles', { types: { refactor: 'max', triage: 'none' } });
    expect(s.types).toEqual({ refactor: 'max', triage: 'none' });
  });

  it('stores the task type, lets its profile fill the effort and clears it on patch', async () => {
    const d = new MockDaemon();
    await d.request('PUT', '/v1/profiles', { types: { refactor: 'max' } });
    const t = await d.request<Task>('POST', '/v1/tasks', { title: 'migrate', description: '', project: '/w/app', agent: 'claude', model: '', taskType: 'refactor' });
    expect(t.taskType).toBe('refactor');
    expect(t.effort).toBe('');
    const plain = await d.request<Task>('POST', '/v1/tasks', { title: 'x', description: '', project: '/w/app', agent: 'claude', model: '' });
    expect(plain.effort).toBe('high');
    expect(plain.taskType).toBeUndefined();
    await expect(d.request('POST', '/v1/tasks', { title: 'x', description: '', project: '/w/app', agent: 'claude', model: '', taskType: 'chore' })).rejects.toThrow('taskType must be one of fix, feature, engineering, refactor, triage');
    const typedTask = await d.request<Task>('PATCH', `/v1/tasks/${plain.id}`, { taskType: 'fix' });
    expect(typedTask.taskType).toBe('fix');
    const cleared = await d.request<Task>('PATCH', `/v1/tasks/${plain.id}`, { taskType: null });
    expect(cleared.taskType).toBeUndefined();
    await expect(d.request('PATCH', `/v1/tasks/${plain.id}`, { taskType: 'chore' })).rejects.toThrow(/taskType must be one of/);
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
