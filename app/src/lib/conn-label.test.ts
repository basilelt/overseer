import { describe, expect, it } from 'vitest';
import { connLabel, HOST_CHIP_DOT, phoneHostDot } from './conn-label';

describe('host dot colours', () => {
  it('shows every connecting phase as working on the desktop chip', () => {
    expect(HOST_CHIP_DOT.connecting).toBe('var(--st-working)');
    expect(HOST_CHIP_DOT.reconnecting).toBe('var(--st-working)');
    expect(HOST_CHIP_DOT.idle).toBe('var(--st-held)');
    expect(HOST_CHIP_DOT.connected).toBe('var(--st-input)');
    expect(HOST_CHIP_DOT.error).toBe('var(--st-failed)');
  });

  it('shows only connected and error as coloured on the phone', () => {
    expect(phoneHostDot('connected')).toBe('var(--st-input)');
    expect(phoneHostDot('error')).toBe('var(--st-failed)');
    expect(phoneHostDot('connecting')).toBe('var(--st-held)');
    expect(phoneHostDot('reconnecting')).toBe('var(--st-held)');
  });
});

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
