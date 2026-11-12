import { describe, expect, it } from 'vitest';
import { NONE } from './profiles';
import { savePayload } from './profile-edit';
import type { Profile, ProfilesStore } from '../types';

const builtin = { id: 'balanced', name: 'Balanced', builtin: true, agents: {} } as Profile;
const mine = (id: string, name = id): Profile => ({ id, name, agents: {} });
const store = (over: Partial<ProfilesStore> = {}): ProfilesStore => ({ default: 'balanced', profiles: [builtin, mine('old')], scopes: {}, support: {}, ...over });

describe('savePayload', () => {
  it('trims names and drops empty agent settings', () => {
    const draft = [{ id: 'a', name: ' Fast ', agents: { claude: { model: 'haiku', effort: '' as never }, codex: {} } } as Profile];
    expect(savePayload(store(), draft).profiles).toEqual([{ id: 'a', name: 'Fast', agents: { claude: { model: 'haiku' }, codex: {} } }]);
  });

  it('drops scope and type rules that point at a deleted profile, keeping built-ins, the none marker and empty ids', () => {
    const s = store({ scopes: { '/a': 'old', '/b': 'balanced', '/c': NONE, '/d': 'kept', '/e': '' }, types: { bug: 'old', feature: 'kept' } });
    const out = savePayload(s, [mine('kept')]);
    expect(out.scopes).toEqual({ '/b': 'balanced', '/c': NONE, '/d': 'kept', '/e': '' });
    expect(out.types).toEqual({ feature: 'kept' });
  });

  it('clears a default that points at a deleted profile, and keeps a known one', () => {
    expect(savePayload(store({ default: 'old' }), []).default).toBe('');
    expect(savePayload(store({ default: 'old' }), [mine('old')]).default).toBe('old');
    expect(savePayload(store({ default: '' }), []).default).toBe('');
  });

  it('answers an empty type map when the daemon has none', () => {
    expect(savePayload(store(), []).types).toEqual({});
  });
});
