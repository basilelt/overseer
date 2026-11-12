import { describe, expect, it } from 'vitest';
import { normalizeUrl } from './browser';

describe('normalizeUrl', () => {
  it('adds schemes sensibly', () => {
    expect(normalizeUrl('github.com/acme')).toBe('https://github.com/acme');
    expect(normalizeUrl('localhost:5173')).toBe('http://localhost:5173');
    expect(normalizeUrl('127.0.0.1:8080/x')).toBe('http://127.0.0.1:8080/x');
    expect(normalizeUrl('devbox:3000')).toBe('http://devbox:3000');
    expect(normalizeUrl('http://a.b')).toBe('http://a.b');
    expect(normalizeUrl(' ')).toBe('');
    expect(normalizeUrl('rust traits')).toContain('duckduckgo.com/?q=rust%20traits');
  });
});
