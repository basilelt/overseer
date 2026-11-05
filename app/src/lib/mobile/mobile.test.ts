import { describe, expect, it } from 'vitest';
import { applyMods, ctrlChar, NO_MODS, nextModState, pressKey, type Mods } from './accessory';
import { composeOutcome, composeRequest, composeTarget } from './compose';
import { movedTooFar, pinchFontSize, swipeDirection } from './gestures';
import { columnAfterSwipe, keyboardInset, layoutFor, mobileUserAgent, phoneParam, tabForView } from './layout';
import { canResize, fitTo, isAgentTerm, shouldReassert, sizedElsewhere } from './termsize';

describe('layout', () => {
  it('switches to the phone layout under the breakpoint', () => {
    expect(layoutFor({ width: 390 })).toBe('phone');
    expect(layoutFor({ width: 720 })).toBe('phone');
    expect(layoutFor({ width: 721 })).toBe('desktop');
    expect(layoutFor({ width: 1440 })).toBe('desktop');
    expect(layoutFor({ width: 0 })).toBe('desktop');
  });

  it('measures the keyboard only when the webview itself was not resized', () => {
    expect(keyboardInset(844, 544)).toBe(300);
    expect(keyboardInset(544, 544)).toBe(0);
    expect(keyboardInset(844, 800)).toBe(0);
    expect(keyboardInset(844, 422, 2)).toBe(0);
    expect(keyboardInset(844, 0)).toBe(0);
  });

  it('honours ?phone=1 and mobile systems', () => {
    expect(layoutFor({ width: 1440, forcePhone: true })).toBe('phone');
    expect(layoutFor({ width: 900, mobileOs: true })).toBe('phone');
    expect(layoutFor({ width: 1180, mobileOs: true })).toBe('desktop');
    expect(phoneParam('?phone=1')).toBe(true);
    expect(phoneParam('?theme=dark&phone=true')).toBe(true);
    expect(phoneParam('?phone=0')).toBe(false);
    expect(phoneParam('')).toBe(false);
    expect(mobileUserAgent('Mozilla/5.0 (Linux; Android 15; Pixel 9)')).toBe(true);
    expect(mobileUserAgent('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)')).toBe(true);
    expect(mobileUserAgent('Mozilla/5.0 (X11; Linux x86_64)')).toBe(false);
  });

  it('maps views to bottom tabs', () => {
    expect(tabForView('agents', 'line:a')).toBe('agents');
    expect(tabForView('agents', 'review:wt:/x')).toBe('review');
    expect(tabForView('kanban', null)).toBe('tasks');
    expect(tabForView('browser', null)).toBe('more');
  });

  it('keeps swiped columns in range', () => {
    expect(columnAfterSwipe(0, 5, -1)).toBe(0);
    expect(columnAfterSwipe(0, 5, 1)).toBe(1);
    expect(columnAfterSwipe(4, 5, 1)).toBe(4);
    expect(columnAfterSwipe(2, 5, 0)).toBe(2);
  });
});

describe('gestures', () => {
  it('reads horizontal swipes only', () => {
    expect(swipeDirection(-120, 10)).toBe(1);
    expect(swipeDirection(120, -5)).toBe(-1);
    expect(swipeDirection(30, 0)).toBe(0);
    expect(swipeDirection(-100, 90)).toBe(0);
  });

  it('cancels a long press when the finger moves', () => {
    expect(movedTooFar(3, 4)).toBe(false);
    expect(movedTooFar(12, 0)).toBe(true);
  });

  it('scales the terminal font with a pinch, within bounds', () => {
    expect(pinchFontSize(13, 100, 200)).toBe(26);
    expect(pinchFontSize(13, 100, 50)).toBe(8);
    expect(pinchFontSize(13, 100, 400)).toBe(28);
    expect(pinchFontSize(13, 100, 115)).toBe(15);
    expect(pinchFontSize(13, 0, 100)).toBe(13);
  });
});

describe('accessory keys', () => {
  it('sends escape sequences', () => {
    expect(pressKey('esc', NO_MODS).send).toBe('\x1b');
    expect(pressKey('tab', NO_MODS).send).toBe('\t');
    expect(pressKey('up', NO_MODS).send).toBe('\x1b[A');
    expect(pressKey('left', NO_MODS).send).toBe('\x1b[D');
    expect(pressKey('pipe', NO_MODS).send).toBe('|');
    expect(pressKey('tilde', NO_MODS).send).toBe('~');
    expect(pressKey('newline', NO_MODS).send).toBe('\x1b\r');
  });

  it('makes Ctrl sticky for one key, locked on double tap', () => {
    let m: Mods = NO_MODS;
    let p = pressKey('ctrl', m);
    expect(p.send).toBeNull();
    m = p.mods;
    expect(m.ctrl).toBe('once');
    p = applyMods('c', m);
    expect(p.send).toBe('\x03');
    expect(p.mods.ctrl).toBe('off');
    expect(applyMods('c', p.mods).send).toBe('c');

    m = pressKey('ctrl', NO_MODS, true).mods;
    expect(m.ctrl).toBe('locked');
    p = applyMods('d', m);
    expect(p.send).toBe('\x04');
    expect(p.mods.ctrl).toBe('locked');
    expect(pressKey('ctrl', p.mods).mods.ctrl).toBe('off');
    expect(nextModState('once', false)).toBe('off');
  });

  it('combines Ctrl and Alt with arrows and characters', () => {
    expect(pressKey('right', { ctrl: 'once', alt: 'off' }).send).toBe('\x1b[1;5C');
    expect(pressKey('right', { ctrl: 'once', alt: 'off' }).mods.ctrl).toBe('off');
    expect(pressKey('up', { ctrl: 'off', alt: 'once' }).send).toBe('\x1b[1;3A');
    expect(applyMods('b', { ctrl: 'off', alt: 'once' }).send).toBe('\x1bb');
    expect(applyMods('x', { ctrl: 'once', alt: 'once' }).send).toBe('\x1b\x18');
    expect(pressKey('dash', { ctrl: 'once', alt: 'off' }).send).toBe('\x1f');
    expect(applyMods('abc', { ctrl: 'once', alt: 'off' }).send).toBe('\x01bc');
    expect(ctrlChar('[')).toBe('\x1b');
    expect(ctrlChar(' ')).toBe('\x00');
    expect(ctrlChar('é')).toBeNull();
  });
});

describe('compose', () => {
  it('targets the selected session', () => {
    expect(composeTarget('line:a1')).toEqual({ lineId: 'a1' });
    expect(composeTarget('term:t1')).toEqual({ termId: 't1' });
    expect(composeTarget('review:wt:/x')).toBeNull();
    expect(composeTarget(null)).toBeNull();
  });

  it('queues by default and can type now', () => {
    expect(composeRequest('  fix the tests\nthen push  ', { lineId: 'a1' }, 'queue')).toEqual({
      text: 'fix the tests\nthen push',
      lineIds: ['a1'],
      onlyIdle: true,
      queue: true,
    });
    expect(composeRequest('go', { termId: 't1' }, 'now')).toEqual({ text: 'go', termIds: ['t1'], onlyIdle: false });
    expect(composeRequest('   ', { lineId: 'a1' }, 'queue')).toBeNull();
    expect(composeRequest('x', null, 'queue')).toBeNull();
    expect(composeRequest('x'.repeat(64 * 1024 + 1), { lineId: 'a1' }, 'queue')).toBeNull();
  });

  it('reports the delivery', () => {
    expect(composeOutcome([{ target: 'line:a1', result: 'sent' }])).toEqual({ kind: 'sent' });
    expect(composeOutcome([{ target: 'line:a1', result: 'queued' }])).toEqual({ kind: 'queued' });
    expect(composeOutcome([{ target: 'line:a1', result: 'refused', reason: 'not running' }])).toEqual({ kind: 'failed', reason: 'not running' });
    expect(composeOutcome([])).toEqual({ kind: 'failed', reason: 'no target' });
  });
});

describe('terminal size between clients', () => {
  it('tells who resized the shared terminal', () => {
    expect(sizedElsewhere({ cols: 120, rows: 40, sizedBy: 'desktop' }, { cols: 120, rows: 40 }, 'desktop')).toBeNull();
    expect(sizedElsewhere({ cols: 46, rows: 30, sizedBy: 'phone' }, { cols: 120, rows: 40 }, 'desktop')).toBe('phone');
    expect(sizedElsewhere({ cols: 120, rows: 40, sizedBy: 'desktop' }, { cols: 46, rows: 30 }, 'phone')).toBe('desktop');
    expect(sizedElsewhere({ cols: 80, rows: 24 }, { cols: 120, rows: 40 }, 'desktop')).toBe('other');
    expect(sizedElsewhere({ cols: 100, rows: 40, sizedBy: 'desktop' }, { cols: 120, rows: 40 }, 'desktop')).toBeNull();
    expect(sizedElsewhere(undefined, { cols: 1, rows: 1 }, 'phone')).toBeNull();
  });

  it('re-asserts only when this client is focused and visible', () => {
    expect(shouldReassert(true, true, 'phone')).toBe(true);
    expect(shouldReassert(true, false, 'phone')).toBe(false);
    expect(shouldReassert(false, true, 'phone')).toBe(false);
    expect(shouldReassert(true, true, null)).toBe(false);
    expect(shouldReassert(true, true, 'desktop', false)).toBe(false);
  });

  it('never lets a phone resize an agent terminal', () => {
    expect(canResize('phone', true)).toBe(false);
    expect(canResize('phone', false)).toBe(true);
    expect(canResize('desktop', true)).toBe(true);
    expect(isAgentTerm({ lineId: 'ln_1' })).toBe(true);
    expect(isAgentTerm({ agent: 'claude' })).toBe(true);
    expect(isAgentTerm({ lineId: null })).toBe(false);
    expect(sizedElsewhere({ cols: 120, rows: 40, sizedBy: 'desktop', lineId: 'ln_1' }, { cols: 46, rows: 30 }, 'phone')).toBeNull();
    expect(sizedElsewhere({ cols: 120, rows: 40, sizedBy: 'desktop', agent: 'codex' }, { cols: 46, rows: 30 }, 'phone')).toBeNull();
    expect(sizedElsewhere({ cols: 46, rows: 30, sizedBy: 'phone', lineId: 'ln_1' }, { cols: 120, rows: 40 }, 'desktop')).toBe('phone');
  });
});

describe('fitTo', () => {
  const term = (cols: number, rows: number) => {
    const calls: [number, number][] = [];
    return { cols, rows, calls, resize(c: number, r: number) { calls.push([c, r]); this.cols = c; this.rows = r; } };
  };

  it('resizes when the proposal differs from the terminal, even if it matches an earlier proposal', () => {
    const t = term(88, 40);
    expect(fitTo(t, { cols: 46, rows: 40 })).toBe(true);
    expect(t.calls).toEqual([[46, 40]]);
  });

  it('does nothing when the terminal already has the proposed size', () => {
    const t = term(46, 40);
    expect(fitTo(t, { cols: 46, rows: 40 })).toBe(false);
    expect(t.calls).toEqual([]);
  });

  it('does nothing without a proposal', () => {
    const t = term(46, 40);
    expect(fitTo(t, undefined)).toBe(false);
    expect(t.calls).toEqual([]);
  });
});
