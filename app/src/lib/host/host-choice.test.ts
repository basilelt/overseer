import { describe, expect, it } from 'vitest';
import { connectHow, preferredChoice, toggled, type HostPrefs } from './host-choice';

const prefs = (over: Partial<HostPrefs> = {}): HostPrefs => ({ hostWindows: 'separate', hostAsk: false, hostChoice: {}, ...over });
const prod = { kind: 'ssh' as const, host: 'prod' };
const here = { active: 'local', keys: ['local'], live: true };

describe('connectHow', () => {
  it('replaces the host of the window by default, as before', () => {
    expect(connectHow(prefs(), prod, here)).toBe('replace');
  });

  it('adds the host beside the others in one-window mode', () => {
    expect(connectHow(prefs({ hostWindows: 'shared' }), prod, here)).toBe('add');
  });

  it('asks when the user wants to choose each time', () => {
    expect(connectHow(prefs({ hostAsk: true }), prod, here)).toBe('ask');
  });

  it('uses the answer remembered for that host over the mode and the question', () => {
    expect(connectHow(prefs({ hostAsk: true, hostChoice: { 'ssh:prod': 'window' } }), prod, here)).toBe('window');
    expect(connectHow(prefs({ hostChoice: { 'ssh:prod': 'add' } }), prod, here)).toBe('add');
  });

  it('switches to a host already in the window and reconnects the active one in place', () => {
    expect(connectHow(prefs({ hostAsk: true }), prod, { active: 'local', keys: ['local', 'ssh:prod'], live: true })).toBe('switch');
    expect(connectHow(prefs({ hostAsk: true }), prod, { active: 'ssh:prod', keys: ['ssh:prod'], live: false })).toBe('replace');
  });

  it('keeps adding to a window that already shows several hosts, even in one-window-per-host mode', () => {
    expect(connectHow(prefs(), { kind: 'ssh', host: 'stage' }, { active: 'local', keys: ['local', 'ssh:prod'], live: true })).toBe('add');
  });

  it('replaces a window with nothing live, whatever the mode', () => {
    expect(connectHow(prefs({ hostWindows: 'shared', hostAsk: true }), prod, { ...here, live: false })).toBe('replace');
  });

  it('follows the "Add a host to this window" entry over every preference', () => {
    expect(connectHow(prefs({ hostChoice: { 'ssh:prod': 'window' } }), prod, here, 'add')).toBe('add');
  });
});

describe('preferredChoice', () => {
  it('puts the mode the user picked first', () => {
    expect(preferredChoice({ hostWindows: 'shared' })).toBe('add');
    expect(preferredChoice({ hostWindows: 'separate' })).toBe('window');
  });
});

describe('toggled', () => {
  it('adds a missing key and removes a present one', () => {
    expect(toggled(undefined, 'a')).toEqual(['a']);
    expect(toggled(['a', 'b'], 'a')).toEqual(['b']);
  });
});
