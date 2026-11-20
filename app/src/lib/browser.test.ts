import { describe, expect, it } from 'vitest';
import { boundsOf, normalizeUrl } from './browser';

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

describe('boundsOf', () => {
  const rect = (left: number, top: number, right: number, bottom: number) => ({ left, top, right, bottom, width: right - left, height: bottom - top });
  const el = (r: ReturnType<typeof rect>) => ({ getBoundingClientRect: () => r }) as unknown as Element;
  const doc = (r: ReturnType<typeof rect> | null) => ({ querySelector: () => (r ? el(r) : null) }) as Pick<Document, 'querySelector'>;
  const host = rect(288, 80, 1440, 900);

  it('is the element box with no toast', () => {
    expect(boundsOf(el(host), doc(null))).toEqual({ x: 288, y: 80, width: 1152, height: 820 });
  });

  it('stops above a toast that overlaps the element', () => {
    expect(boundsOf(el(host), doc(rect(500, 840, 900, 880))).height).toBe(752);
  });

  it('ignores a toast beside or above the element', () => {
    expect(boundsOf(el(host), doc(rect(10, 840, 200, 880))).height).toBe(820);
    expect(boundsOf(el(host), doc(rect(500, 10, 900, 50))).height).toBe(820);
  });

  it('never goes below zero', () => {
    expect(boundsOf(el(host), doc(rect(500, 82, 900, 120))).height).toBe(0);
  });
});
