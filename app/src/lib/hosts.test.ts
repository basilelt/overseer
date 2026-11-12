import { describe, expect, it } from 'vitest';
import { connectHow, hostColor, hostName, ordered, preferredChoice, restoreHosts, toggled, type HostPrefs } from './hosts';
import type { HostTarget } from './types';

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

describe('hostColor', () => {
  it('keeps local green and gives every SSH host a stable lane colour', () => {
    expect(hostColor('local')).toBe('var(--st-input)');
    expect(hostColor('ssh:prod')).toBe(hostColor('ssh:prod'));
    expect(hostColor('ssh:prod')).toMatch(/^var\(--lane-[1-4]\)$/);
    expect(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'].map((h) => hostColor(`ssh:${h}`))).not.toContain('var(--lane-5)');
    const used = new Set(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'].map((h) => hostColor(`ssh:${h}`)));
    expect(used.size).toBeGreaterThan(1);
  });
});

describe('restoreHosts', () => {
  const local: HostTarget = { kind: 'local', label: 'Local' };
  const p: HostTarget = { kind: 'ssh', host: 'prod', label: 'prod' };

  it('brings back the other hosts once, without the first one or phone hosts', () => {
    expect(restoreHosts([local, p, p, { kind: 'direct', host: 'h1', label: 'x' }], local)).toEqual([p]);
    expect(restoreHosts(undefined, local)).toEqual([]);
  });
});

describe('ordered', () => {
  it('keeps the order hosts joined, unknown ones last', () => {
    expect(ordered(['b', 'a'], ['a', 'c', 'b'], (x) => x)).toEqual(['b', 'a', 'c']);
  });
});

describe('toggled', () => {
  it('adds a missing key and removes a present one', () => {
    expect(toggled(undefined, 'a')).toEqual(['a']);
    expect(toggled(['a', 'b'], 'a')).toEqual(['b']);
  });
});

describe('hostName', () => {
  it('names local, then the label, then the host', () => {
    expect(hostName({ kind: 'local', label: '' })).toBe('Local');
    expect(hostName({ kind: 'ssh', host: 'prod', label: 'Prod' })).toBe('Prod');
    expect(hostName({ kind: 'ssh', host: 'prod', label: '' })).toBe('prod');
  });
});
