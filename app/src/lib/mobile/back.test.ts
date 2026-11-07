import { describe, expect, it, vi } from 'vitest';
import { backAction, backHandlers, closeTop, escapeCloses, onBack, pushTab, runBack, setBackRunner, type BackState } from './back';

const root: BackState = { handlers: 0, tour: false, menu: false, diag: false, overlay: false, entry: false, panel: false, drawer: false, subscreen: false, previousTab: null };

describe('android back routing', () => {
  it('minimises only at the root', () => {
    expect(backAction(root)).toBe('minimize');
  });

  it('closes the topmost layer first', () => {
    expect(backAction({ ...root, handlers: 1, overlay: true, drawer: true, subscreen: true, previousTab: 'tasks' })).toBe('handler');
    expect(backAction({ ...root, tour: true, handlers: 2 })).toBe('tour');
    expect(backAction({ ...root, menu: true, handlers: 1 })).toBe('menu');
    expect(backAction({ ...root, diag: true, overlay: true })).toBe('diag');
    expect(backAction({ ...root, overlay: true, panel: true, drawer: true })).toBe('overlay');
    expect(backAction({ ...root, panel: true, drawer: true })).toBe('panel');
    expect(backAction({ ...root, drawer: true, subscreen: true })).toBe('drawer');
  });

  it('minimises from the first connect screen instead of leaving an empty shell', () => {
    expect(backAction({ ...root, overlay: true, entry: true, previousTab: 'tasks' })).toBe('minimize');
    expect(backAction({ ...root, overlay: true, entry: true, handlers: 1 })).toBe('handler');
  });

  it('leaves a sub-screen before switching tabs', () => {
    expect(backAction({ ...root, subscreen: true, previousTab: 'agents' })).toBe('subscreen');
    expect(backAction({ ...root, previousTab: 'agents' })).toBe('tab');
  });

  it('closes the most recently opened sheet and forgets closed ones', () => {
    const first = vi.fn();
    const second = vi.fn();
    const offFirst = onBack(first);
    const offSecond = onBack(second);
    expect(backHandlers()).toBe(2);
    expect(closeTop()).toBe(true);
    expect(second).toHaveBeenCalledOnce();
    expect(first).not.toHaveBeenCalled();
    offSecond();
    expect(closeTop()).toBe(true);
    expect(first).toHaveBeenCalledOnce();
    offFirst();
    offFirst();
    expect(backHandlers()).toBe(0);
    expect(closeTop()).toBe(false);
  });

  it('runs the shell router only while it is mounted', () => {
    expect(runBack()).toBe(false);
    const run = vi.fn();
    const stop = setBackRunner(run);
    expect(runBack()).toBe(true);
    expect(run).toHaveBeenCalledOnce();
    stop();
    expect(runBack()).toBe(false);
  });
});

describe('tab history', () => {
  it('returns to the previous tab, never loops and stays bounded', () => {
    let h = pushTab([], 'agents', 'tasks');
    h = pushTab(h, 'tasks', 'review');
    expect(h).toEqual(['agents', 'tasks']);
    h = pushTab(h, 'review', 'agents');
    expect(h).toEqual(['tasks', 'review']);
    expect(pushTab(h, 'agents', 'agents')).toBe(h);
    expect(pushTab(['agents', 'tasks', 'review'], 'more', 'agents', 2)).toEqual(['review', 'more']);
  });
});

describe('Escape on a phone sheet', () => {
  it('closes an open sheet', () => {
    expect(escapeCloses({ key: 'Escape', defaultPrevented: false }, true)).toBe(true);
  });

  it('does nothing when the sheet is closed, for other keys, or when a field already used the key', () => {
    expect(escapeCloses({ key: 'Escape', defaultPrevented: false }, false)).toBe(false);
    expect(escapeCloses({ key: 'Enter', defaultPrevented: false }, true)).toBe(false);
    expect(escapeCloses({ key: 'Escape', defaultPrevented: true }, true)).toBe(false);
  });

  it('is wired to the Activity sheet of the phone shell', async () => {
    const src = (await import('../components/mobile/MobileShell.svelte?raw')).default;
    expect(src).toMatch(/<svelte:window onkeydown=\{[^}]*escapeCloses\(e, ui\.panel\)[^}]*ui\.panel = false/);
  });
});
