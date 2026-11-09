import { describe as group, expect, it } from 'vitest';
import { activeAt, canVideo, clock, describe, extFor, fileName, formatLog, isField, pickMime, shortcut, type RecEvent } from './record';
import { el } from '../test/fake-dom';

const asEl = (x: unknown) => x as Element;
const key = (k: string, mods: Partial<Record<'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey', boolean>> = {}) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

group('clock', () => {
  it('formats minutes and zero-padded seconds', () => {
    expect(clock(0)).toBe('0:00');
    expect(clock(9_999)).toBe('0:09');
    expect(clock(125_000)).toBe('2:05');
  });

  it('clamps negative times to zero', () => {
    expect(clock(-5)).toBe('0:00');
  });
});

group('describe', () => {
  it('names a button by its aria-label', () => {
    expect(describe(asEl(el('button', { 'aria-label': 'Close' }, 'x')))).toBe('button "Close"');
  });

  it('climbs from an inner icon to the control and uses its text', () => {
    const btn = el('button', {}, '  Send\n feedback ');
    expect(describe(asEl(el('svg').in(btn)))).toBe('button "Send feedback"');
  });

  it('prefers the role attribute over the tag', () => {
    expect(describe(asEl(el('div', { role: 'tab', title: 'Kanban' })))).toBe('tab "Kanban"');
  });

  it('never reads the text of a field, only its label', () => {
    expect(describe(asEl(el('textarea', { placeholder: 'Describe' }, 'my password is hunter2')))).toBe('textarea "Describe"');
    expect(describe(asEl(el('input', { type: 'text' }, 'secret')))).toBe('input field');
  });

  it('redacts secrets in labels and clips long ones', () => {
    expect(describe(asEl(el('button', { title: 'token=abcdef123456' })))).toBe('button "token=***"');
    const d = describe(asEl(el('button', {}, 'x'.repeat(200))))!;
    expect(d.length).toBeLessThan(80);
    expect(d.endsWith('…"')).toBe(true);
  });

  it('returns null for an unnamed element and for no target', () => {
    expect(describe(asEl(el('div')))).toBeNull();
    expect(describe(null)).toBeNull();
  });
});

group('isField', () => {
  it('treats text inputs, textareas, contenteditable and terminals as fields', () => {
    expect(isField(asEl(el('input', { type: 'text' })))).toBe(true);
    expect(isField(asEl(el('textarea')))).toBe(true);
    const ce = el('div');
    ce.isContentEditable = true;
    expect(isField(asEl(ce))).toBe(true);
    expect(isField(asEl(el('canvas').in(el('div', { 'data-terminal': '' }))))).toBe(true);
    expect(isField(asEl(el('div').in(el('div', { 'data-terminal': '' }))))).toBe(true);
  });

  it('does not treat buttons and checkboxes as fields', () => {
    expect(isField(asEl(el('input', { type: 'checkbox' })))).toBe(false);
    expect(isField(asEl(el('button')))).toBe(false);
    expect(isField(null)).toBe(false);
  });
});

group('shortcut', () => {
  it('logs modifier shortcuts with an upper-case key', () => {
    expect(shortcut(key('p', { ctrlKey: true }), false)).toBe('Ctrl+P');
    expect(shortcut(key('k', { metaKey: true, shiftKey: true }), true)).toBe('Cmd+Shift+K');
  });

  it('logs named keys outside fields', () => {
    expect(shortcut(key('Escape'), false)).toBe('Escape');
    expect(shortcut(key('ArrowDown'), false)).toBe('ArrowDown');
    expect(shortcut(key('F5'), false)).toBe('F5');
  });

  it('never logs plain typing, even with Shift', () => {
    expect(shortcut(key('a'), false)).toBeNull();
    expect(shortcut(key('A', { shiftKey: true }), false)).toBeNull();
    expect(shortcut(key('a'), true)).toBeNull();
  });

  it('logs only Escape, function keys and real shortcuts inside fields', () => {
    expect(shortcut(key('Enter'), true)).toBeNull();
    expect(shortcut(key('Backspace'), true)).toBeNull();
    expect(shortcut(key('Escape'), true)).toBe('Escape');
    expect(shortcut(key('F2'), true)).toBe('F2');
    expect(shortcut(key('Enter', { ctrlKey: true }), true)).toBe('Ctrl+Enter');
  });

  it('ignores a lone modifier', () => {
    expect(shortcut(key('Control', { ctrlKey: true }), false)).toBeNull();
    expect(shortcut(key('Shift', { shiftKey: true }), false)).toBeNull();
  });
});

group('formatLog', () => {
  it('prints one aligned line per event and redacts it', () => {
    const events: RecEvent[] = [
      { at: 0, kind: 'start', text: 'Recording started' },
      { at: 61_000, kind: 'error', text: 'fetch https://u:pw@h/x failed' },
    ];
    expect(formatLog(events)).toBe('0:00  start  Recording started\n1:01  error  fetch https://u:***@h/x failed');
  });

  it('is empty for no events', () => {
    expect(formatLog([])).toBe('');
  });
});

group('activeAt', () => {
  const events: RecEvent[] = [0, 1000, 1000, 5000].map((at) => ({ at, kind: 'click', text: '' }));

  it('returns the last event at or before the time', () => {
    expect(activeAt(events, 0)).toBe(0);
    expect(activeAt(events, 1500)).toBe(2);
    expect(activeAt(events, 99_000)).toBe(3);
  });

  it('returns -1 before the first event or with no events', () => {
    expect(activeAt(events, -1)).toBe(-1);
    expect(activeAt([], 10)).toBe(-1);
  });
});

group('media helpers', () => {
  it('picks the first supported format', () => {
    expect(pickMime((m) => m === 'video/webm')).toBe('video/webm');
    expect(pickMime((m) => m === 'video/mp4')).toBe('video/mp4');
    expect(pickMime(() => false)).toBe('');
  });

  it('maps the format to a GitHub-accepted extension', () => {
    expect(extFor('video/mp4;codecs=avc1')).toBe('mp4');
    expect(extFor('video/webm;codecs=vp9')).toBe('webm');
    expect(extFor('')).toBe('webm');
  });

  it('names the file after the local time', () => {
    expect(fileName(new Date(2026, 0, 2, 3, 4, 5), 'video/webm')).toBe('recording-20260102-030405.webm');
  });

  it('detects screen recording support', () => {
    expect(canVideo({ navigator: { mediaDevices: { getDisplayMedia: () => {} } }, MediaRecorder: class {} } as unknown as typeof globalThis)).toBe(true);
    expect(canVideo({ navigator: { mediaDevices: {} }, MediaRecorder: class {} } as unknown as typeof globalThis)).toBe(false);
    expect(canVideo({ navigator: { mediaDevices: { getDisplayMedia: () => {} } } } as unknown as typeof globalThis)).toBe(false);
    expect(canVideo({} as typeof globalThis)).toBe(false);
  });
});
