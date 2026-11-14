import { describe, expect, it, vi } from 'vitest';
import * as ghostty from 'ghostty-web';
import { BACKGROUND_FRAME_MS, BLINK_MS, INACTIVE_FRAME_MS, canPatch, FrameScheduler, installOnDemandRendering, type Clock, type PaintTarget } from './termrender';
import type { Presence } from '../presence';

function fakeClock() {
  const frames: ((now: number) => void)[] = [];
  const timers = new Map<number, () => void>();
  let id = 0;
  const clock: Clock = {
    frame: (cb) => frames.push(cb),
    every: (fn) => {
      timers.set(++id, fn);
      return id;
    },
    cancel: (h) => timers.delete(h as number),
  };
  return {
    clock,
    frames,
    timers,
    run(now: number) {
      const due = frames.splice(0);
      for (const f of due) f(now);
      return due.length;
    },
    tick() {
      for (const fn of [...timers.values()]) fn();
    },
  };
}

function target(): PaintTarget & { renders: number } {
  const t = {
    renders: 0,
    viewportY: 0,
    scrollbarOpacity: 0,
    lastCursorY: 0,
    isDisposed: false,
    isOpen: true,
    wasmTerm: { getCursor: () => ({ y: 0 }) },
    renderer: {
      cursorVisible: true,
      stopCursorBlink: () => {},
      render: () => {
        t.renders++;
      },
    },
  };
  return t;
}

function setup(p: Presence = { visible: true, focused: true }) {
  const c = fakeClock();
  const state = { ...p };
  const s = new FrameScheduler(c.clock, () => state);
  return { c, s, state };
}

describe('FrameScheduler', () => {
  it('paints once on add and then stays idle without a single pending frame', () => {
    const { c, s } = setup();
    const t = target();
    s.add(t);
    expect(c.run(0)).toBe(1);
    expect(t.renders).toBe(1);
    expect(c.frames.length).toBe(0);
    expect(c.run(16)).toBe(0);
    expect(t.renders).toBe(1);
  });

  it('coalesces many invalidations into one paint per frame', () => {
    const { c, s } = setup();
    const t = target();
    s.add(t);
    s.setFocus(t);
    c.run(0);
    for (let i = 0; i < 50; i++) s.invalidate(t);
    expect(c.frames.length).toBe(1);
    c.run(16);
    expect(t.renders).toBe(2);
  });

  it('shares one frame across terminals', () => {
    const { c, s } = setup();
    const a = target();
    const b = target();
    s.add(a);
    s.add(b);
    expect(c.frames.length).toBe(1);
    c.run(0);
    expect([a.renders, b.renders]).toEqual([1, 1]);
  });

  it('does not schedule frames while the window is hidden and catches up when shown', () => {
    const { c, s, state } = setup({ visible: false, focused: false });
    const t = target();
    s.add(t);
    s.invalidate(t);
    expect(c.frames.length).toBe(0);
    state.visible = true;
    s.presenceChanged();
    c.run(0);
    expect(t.renders).toBe(1);
  });

  it('caps background terminals at 30 fps', () => {
    const { c, s } = setup();
    const t = target();
    s.add(t);
    c.run(0);
    s.invalidate(t);
    c.run(16);
    expect(t.renders).toBe(1);
    expect(c.frames.length).toBe(1);
    c.run(BACKGROUND_FRAME_MS + 1);
    expect(t.renders).toBe(2);
  });

  it('paints the focused terminal every frame', () => {
    const { c, s } = setup();
    const t = target();
    s.add(t);
    s.setFocus(t);
    c.run(0);
    s.invalidate(t);
    c.run(16);
    expect(t.renders).toBe(2);
  });

  it('skips terminals that are off screen until they come back', () => {
    const { c, s } = setup();
    const t = target();
    s.add(t);
    c.run(0);
    s.setOnScreen(t, false);
    s.invalidate(t);
    c.run(100);
    expect(t.renders).toBe(1);
    expect(c.frames.length).toBe(0);
    s.setOnScreen(t, true);
    c.run(200);
    expect(t.renders).toBe(2);
  });

  it('reports when a terminal stops and starts being shown', () => {
    const { c, s, state } = setup();
    const t = target();
    s.add(t);
    c.run(0);
    const seen: boolean[] = [];
    const stop = s.watch(t, (v) => seen.push(v));
    expect(s.shown(t)).toBe(true);
    s.setOnScreen(t, false);
    s.setOnScreen(t, false);
    expect(seen).toEqual([false]);
    s.setOnScreen(t, true);
    state.visible = false;
    s.presenceChanged();
    state.focused = false;
    s.presenceChanged();
    expect(seen).toEqual([false, true, false]);
    expect(s.shown(t)).toBe(false);
    stop();
    state.visible = true;
    s.presenceChanged();
    expect(seen).toEqual([false, true, false]);
    expect(s.watch(target(), () => {})).toBeTypeOf('function');
  });

  it('caps every terminal at a low frame rate while the window is unfocused', () => {
    const { c, s, state } = setup();
    const t = target();
    s.add(t);
    s.setFocus(t);
    c.run(0);
    state.focused = false;
    s.presenceChanged();
    s.invalidate(t);
    c.run(16);
    expect(t.renders).toBe(1);
    c.run(INACTIVE_FRAME_MS);
    expect(t.renders).toBe(2);
    state.focused = true;
    s.presenceChanged();
    s.invalidate(t);
    c.run(INACTIVE_FRAME_MS + 16);
    expect(t.renders).toBe(3);
    expect(INACTIVE_FRAME_MS).toBeGreaterThan(BACKGROUND_FRAME_MS);
  });

  it('blinks only the focused terminal, only while the window is active', () => {
    const { c, s, state } = setup();
    const t = target();
    s.add(t);
    c.run(0);
    expect(s.blinking()).toBe(false);
    s.setFocus(t);
    expect(s.blinking()).toBe(true);
    c.tick();
    expect(t.renderer!.cursorVisible).toBe(false);
    state.focused = false;
    s.presenceChanged();
    expect(s.blinking()).toBe(false);
    expect(t.renderer!.cursorVisible).toBe(true);
    expect(c.timers.size).toBe(0);
    state.focused = true;
    s.presenceChanged();
    expect(s.blinking()).toBe(true);
    s.setFocus(null);
    expect(s.blinking()).toBe(false);
    expect(BLINK_MS).toBeGreaterThanOrEqual(500);
  });

  it('shows the cursor solid again when typing', () => {
    const { c, s } = setup();
    const t = target();
    s.add(t);
    s.setFocus(t);
    c.tick();
    expect(t.renderer!.cursorVisible).toBe(false);
    s.typed(t);
    expect(t.renderer!.cursorVisible).toBe(true);
    expect(c.timers.size).toBe(1);
  });

  it('forgets removed or disposed terminals and runs their cleanup', () => {
    const { c, s } = setup();
    const t = target();
    const off = vi.fn();
    s.add(t);
    s.onCleanup(t, off);
    s.setFocus(t);
    s.remove(t);
    expect(off).toHaveBeenCalledOnce();
    expect(s.size).toBe(0);
    expect(s.blinking()).toBe(false);
    s.invalidate(t);
    c.run(0);
    expect(t.renders).toBe(0);
    const d = target();
    s.add(d);
    d.isDisposed = true;
    c.run(0);
    expect(d.renders).toBe(0);
  });

  it('keeps painting other terminals when one render throws', () => {
    const { c, s } = setup();
    const bad = target();
    bad.renderer!.render = () => {
      throw new Error('boom');
    };
    const good = target();
    const err = vi.spyOn(console, 'error').mockImplementation(() => {});
    s.add(bad);
    s.add(good);
    c.run(0);
    expect(good.renders).toBe(1);
    err.mockRestore();
  });
});

describe('ghostty-web patch', () => {
  it('matches the pinned ghostty-web internals', () => {
    expect(canPatch(ghostty)).toBe(true);
  });

  it('replaces the perpetual render loop with on-demand frames', () => {
    const { c, s } = setup();
    class Sel {
      terminal?: PaintTarget;
      requestRender() {}
    }
    class Ren {
      render() {}
      stopCursorBlink() {}
      setHoveredHyperlinkId() {}
      setHoveredLinkRange() {}
    }
    const loop = vi.fn();
    class Term {
      startRenderLoop() {
        loop();
      }
      writeInternal() {}
      dispose() {}
    }
    expect(installOnDemandRendering({ Terminal: Term, SelectionManager: Sel, CanvasRenderer: Ren }, s)).toBe(true);
    const base = target();
    const t = Object.assign(new Term(), base) as unknown as PaintTarget & Term;
    t.startRenderLoop();
    expect(loop).not.toHaveBeenCalled();
    c.run(0);
    expect(base.renders).toBe(1);
    t.writeInternal();
    c.run(100);
    expect(base.renders).toBe(2);
    const sel = new Sel();
    sel.terminal = t;
    sel.requestRender();
    c.run(200);
    expect(base.renders).toBe(3);
    t.dispose();
    expect(s.size).toBe(0);
  });

  it('leaves unknown versions alone', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(installOnDemandRendering({ Terminal: class {} })).toBe(false);
    warn.mockRestore();
  });

  it('attaches once when ghostty restarts its render loop after a resize', () => {
    const { s } = setup();
    class Sel {
      requestRender() {}
    }
    class Ren {
      render() {}
      stopCursorBlink() {}
      setHoveredHyperlinkId() {}
      setHoveredLinkRange() {}
    }
    class Term {
      startRenderLoop() {}
      writeInternal() {}
      dispose() {}
    }
    installOnDemandRendering({ Terminal: Term, SelectionManager: Sel, CanvasRenderer: Ren }, s);
    const onScroll = vi.fn(() => ({ dispose() {} }));
    const t = Object.assign(new Term(), target(), { onScroll }) as unknown as PaintTarget & Term;
    t.startRenderLoop();
    t.startRenderLoop();
    expect(onScroll).toHaveBeenCalledTimes(1);
    t.dispose();
    t.startRenderLoop();
    expect(onScroll).toHaveBeenCalledTimes(2);
  });
});
