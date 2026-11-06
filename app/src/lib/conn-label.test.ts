import { describe, expect, it } from 'vitest';
import { connLabel } from './conn-label';

describe('connLabel', () => {
  it('fills the attempt of a reconnection', () => {
    // Given a reconnecting host on its 3rd attempt
    // When the label is built
    // Then no placeholder is left
    expect(connLabel({ phase: 'reconnecting', attempt: 3 })).toBe('Reconnecting (attempt 3)');
    expect(connLabel({ phase: 'reconnecting' })).toBe('Reconnecting (attempt 1)');
  });

  it('fills the version of an install', () => {
    // Given an installing host
    // When the label is built
    // Then the version is in the text
    expect(connLabel({ phase: 'installing', version: '1.2.3' })).toBe('Installing server v1.2.3');
  });

  it('uses the plain phase text otherwise', () => {
    // Given a connected host
    // When the label is built
    // Then it reads Connected
    expect(connLabel({ phase: 'connected' })).toBe('Connected');
  });
});
