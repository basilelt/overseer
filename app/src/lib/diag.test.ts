import { describe, expect, it } from 'vitest';
import { describeExit, errorText, filterLines, forwardWarning, Limiter, parseLines, type PreviousSession } from './diag';

const LINES = [
  '2026-10-29T10:00:00.000Z INFO  app: Overseer 0.4.0 starting',
  '2026-10-29T10:00:01.000Z WARN  ui: UI unresponsive for 6 s (view: agents)',
  '2026-10-29T10:00:02.000Z ERROR webview: TypeError: x is undefined',
  '    at render (App.svelte:10:2)',
  '2026-10-29T10:00:03.000Z DEBUG conn: noise',
  '2026-10-29T10:00:04.000Z INFO listening on /tmp/s.sock',
  "thread 'main' panicked at src/x.rs:1:1:",
];

const previous: PreviousSession = {
  kind: 'freeze',
  startedAt: Date.UTC(2026, 9, 29, 9, 0, 0),
  endedAt: '2026-10-29T10:00:04.000Z',
  detectedAt: Date.UTC(2026, 9, 29, 11, 0, 0),
  version: '0.4.0',
  frozenSecs: 14,
  view: 'agents/term/connected',
  crashReport: null,
  appLog: LINES.slice(0, 2),
};

describe('log parsing and filtering', () => {
  it('reads levels and keeps continuation lines with their entry', () => {
    const parsed = parseLines(LINES);
    expect(parsed.map((l) => l.level)).toEqual(['INFO', 'WARN', 'ERROR', 'ERROR', 'DEBUG', 'INFO', 'ERROR']);
  });

  it('filters by minimum level and case-insensitive search', () => {
    const parsed = parseLines(LINES);
    expect(filterLines(parsed, 'all', '')).toHaveLength(7);
    expect(filterLines(parsed, 'info', '')).toHaveLength(6);
    expect(filterLines(parsed, 'warn', '').map((l) => l.level)).toEqual(['WARN', 'ERROR', 'ERROR', 'ERROR']);
    expect(filterLines(parsed, 'error', 'typeerror')).toHaveLength(1);
    expect(filterLines(parsed, 'all', '  UNRESPONSIVE ')).toHaveLength(1);
  });
});

describe('diagnostic bundle', () => {
  it('describes how the previous session ended', () => {
    expect(describeExit(previous)).toBe('was frozen (UI unresponsive) when it ended, unresponsive for 14 s, view: agents/term/connected');
    expect(describeExit({ ...previous, kind: 'killed', view: '' })).toBe('was killed or force-quit (no clean exit)');
  });
});

describe('frontend error forwarding', () => {
  it('turns any thrown value into a message', () => {
    const e = new TypeError('bad');
    expect(errorText(e)).toEqual({ message: 'TypeError: bad', stack: e.stack });
    expect(errorText('plain')).toEqual({ message: 'plain' });
    expect(errorText({ code: 1 })).toEqual({ message: '{"code":1}' });
  });

  it('rate-limits bursts and repeated messages, then resets', () => {
    const l = new Limiter(5, 1000);
    expect([1, 2, 3, 4].map(() => l.allow('same', 0))).toEqual([true, true, true, false]);
    expect(['a', 'b', 'c'].map((k) => l.allow(k, 10))).toEqual([true, true, false]);
    expect(l.takeDropped()).toBe(2);
    expect(l.takeDropped()).toBe(0);
    expect(l.allow('same', 1000)).toBe(true);
  });

  it('forwards only the warnings worth keeping', () => {
    expect(forwardWarning('[svelte] ownership_invalid_binding')).toBe(true);
    expect(forwardWarning('ghostty: wasm memory grow failed')).toBe(true);
    expect(forwardWarning('Download the React DevTools')).toBe(false);
  });
});
