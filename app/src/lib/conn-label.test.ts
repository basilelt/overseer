import { describe, expect, it } from 'vitest';
import { connLabel, hostChipDot, HOST_WORKING_DOT } from './conn-label';
import * as hosts from './mobile/hosts';

describe('host dot colours', () => {
  it('shows every connecting phase as working on the desktop chip', () => {
    for (const p of ['connecting', 'checking', 'downloading', 'installing', 'starting', 'reconnecting'] as const) expect(hostChipDot(p)).toBe(HOST_WORKING_DOT);
    expect(hostChipDot('idle')).toBe('var(--st-held)');
    expect(hostChipDot('connected')).toBe('var(--st-input)');
    expect(hostChipDot('error')).toBe('var(--st-failed)');
  });
});

describe('host dot home', () => {
  it('has no phone-specific mapping next to the host form helpers', () => {
    expect(hosts).not.toHaveProperty('phoneHostDot');
  });
});

describe('connLabel', () => {
  it('fills the attempt of a reconnection', () => {
    expect(connLabel({ phase: 'reconnecting', attempt: 3 })).toBe('Reconnecting (attempt 3)');
    expect(connLabel({ phase: 'reconnecting' })).toBe('Reconnecting (attempt 1)');
  });

  it('fills the version of an install', () => {
    expect(connLabel({ phase: 'installing', version: '1.2.3' })).toBe('Installing server v1.2.3');
  });

  it('uses the plain phase text otherwise', () => {
    expect(connLabel({ phase: 'connected' })).toBe('Connected');
  });
});
