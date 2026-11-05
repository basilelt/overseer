import { onPresence, presence, type Presence } from './presence';

export const BLINK_MS = 600;
export const BACKGROUND_FRAME_MS = 1000 / 30;
export const INACTIVE_FRAME_MS = 1000 / 15;

interface Disposable {
  dispose(): void;
}

export interface PaintRenderer {
  render(buffer: unknown, force: boolean, viewportY: number, provider: unknown, scrollbarOpacity: number): void;
  cursorVisible: boolean;
  stopCursorBlink(): void;
  setHoveredHyperlinkId?(id: number): void;
  setHoveredLinkRange?(range: unknown): void;
}

export interface PaintTarget {
  renderer?: PaintRenderer;
  wasmTerm?: { getCursor(): { y: number } };
  viewportY: number;
  scrollbarOpacity: number;
  lastCursorY: number;
  cursorMoveEmitter?: { fire(): void };
  isDisposed: boolean;
  isOpen: boolean;
  element?: HTMLElement;
  onScroll?: (fn: () => void) => Disposable;
  onSelectionChange?: (fn: () => void) => Disposable;
}

export interface Clock {
  frame(cb: (now: number) => void): number;
  every(fn: () => void, ms: number): unknown;
  cancel(handle: unknown): void;
}

interface Entry {
  last: number;
  onScreen: boolean;
  shown: boolean;
  watchers: Set<(shown: boolean) => void>;
  cleanup: (() => void)[];
}

const browserClock: Clock = {
  frame: (cb) => requestAnimationFrame(cb),
  every: (fn, ms) => setInterval(fn, ms),
  cancel: (h) => clearInterval(h as ReturnType<typeof setInterval>),
};

export class FrameScheduler {
  private entries = new Map<PaintTarget, Entry>();
  private dirty = new Set<PaintTarget>();
  private pending = false;
  private focus: PaintTarget | null = null;
  private blink: unknown = null;
  paints = 0;

  constructor(
    private clock: Clock = browserClock,
    private state: () => Readonly<Presence> = presence,
  ) {}

  get size(): number {
    return this.entries.size;
  }

  add(t: PaintTarget): void {
    if (this.entries.has(t)) return;
    this.entries.set(t, { last: -Infinity, onScreen: true, shown: this.state().visible, watchers: new Set(), cleanup: [] });
    this.invalidate(t);
  }

  onCleanup(t: PaintTarget, fn: () => void): void {
    const e = this.entries.get(t);
    if (e) e.cleanup.push(fn);
    else fn();
  }

  remove(t: PaintTarget): void {
    const e = this.entries.get(t);
    if (!e) return;
    this.entries.delete(t);
    this.dirty.delete(t);
    e.watchers.clear();
    if (this.focus === t) this.setFocus(null);
    for (const fn of e.cleanup.splice(0)) {
      try {
        fn();
      } catch {}
    }
  }

  invalidate(t: PaintTarget): void {
    if (!this.entries.has(t)) return;
    this.dirty.add(t);
    this.schedule();
  }

  setOnScreen(t: PaintTarget, on: boolean): void {
    const e = this.entries.get(t);
    if (!e || e.onScreen === on) return;
    e.onScreen = on;
    if (on) this.invalidate(t);
    this.syncBlink();
    this.syncShown(e);
  }

  shown(t: PaintTarget): boolean {
    return this.entries.get(t)?.shown ?? true;
  }

  watch(t: PaintTarget, fn: (shown: boolean) => void): () => void {
    const e = this.entries.get(t);
    if (!e) return () => {};
    e.watchers.add(fn);
    return () => e.watchers.delete(fn);
  }

  private syncShown(e: Entry): void {
    const shown = e.onScreen && this.state().visible;
    if (shown === e.shown) return;
    e.shown = shown;
    for (const fn of [...e.watchers]) {
      try {
        fn(shown);
      } catch {}
    }
  }

  setFocus(t: PaintTarget | null): void {
    if (t && !this.entries.has(t)) return;
    if (this.focus === t) return;
    const prev = this.focus;
    this.focus = t;
    if (prev) this.showCursor(prev);
    this.syncBlink(true);
  }

  focused(): PaintTarget | null {
    return this.focus;
  }

  typed(t: PaintTarget): void {
    if (t !== this.focus) return;
    this.syncBlink(true);
  }

  blinking(): boolean {
    return this.blink !== null;
  }

  presenceChanged(): void {
    this.syncBlink();
    for (const e of this.entries.values()) this.syncShown(e);
    this.schedule();
  }

  private showCursor(t: PaintTarget) {
    if (t.renderer && !t.renderer.cursorVisible) {
      t.renderer.cursorVisible = true;
      this.invalidate(t);
    }
  }

  private syncBlink(restart = false): void {
    const s = this.state();
    const t = this.focus;
    const want = !!t && s.visible && s.focused && !!this.entries.get(t)?.onScreen;
    if (this.blink !== null && (!want || restart)) {
      this.clock.cancel(this.blink);
      this.blink = null;
    }
    if (t) this.showCursor(t);
    if (!want || this.blink !== null || !t) return;
    this.blink = this.clock.every(() => {
      const r = t.renderer;
      if (!r || t.isDisposed) return;
      r.cursorVisible = !r.cursorVisible;
      this.invalidate(t);
    }, BLINK_MS);
  }

  private schedule(): void {
    if (this.pending || !this.dirty.size || !this.state().visible) return;
    this.pending = true;
    this.clock.frame((now) => this.flush(now));
  }

  flush(now: number): void {
    this.pending = false;
    const s = this.state();
    if (!s.visible) return;
    let again = false;
    for (const t of this.dirty) {
      const e = this.entries.get(t);
      if (!e || t.isDisposed || !t.isOpen) {
        this.dirty.delete(t);
        continue;
      }
      if (!e.onScreen) {
        this.dirty.delete(t);
        continue;
      }
      const gap = !s.focused ? INACTIVE_FRAME_MS : t !== this.focus ? BACKGROUND_FRAME_MS : 0;
      if (gap && now - e.last < gap) {
        again = true;
        continue;
      }
      this.dirty.delete(t);
      e.last = now;
      this.paint(t);
    }
    if (again) this.schedule();
  }

  private paint(t: PaintTarget): void {
    const r = t.renderer;
    const buf = t.wasmTerm;
    if (!r || !buf) return;
    this.paints++;
    try {
      r.render(buf, false, t.viewportY, t, t.scrollbarOpacity);
      const c = buf.getCursor();
      if (c.y !== t.lastCursorY) {
        t.lastCursorY = c.y;
        t.cursorMoveEmitter?.fire();
      }
    } catch (err) {
      console.error('terminal render failed', err);
    }
  }
}

export const scheduler = new FrameScheduler();
if (typeof document !== 'undefined') onPresence(() => scheduler.presenceChanged());

const TRIGGERS = [
  'write',
  'writeInternal',
  'resize',
  'clear',
  'reset',
  'focus',
  'blur',
  'clearSelection',
  'selectAll',
  'select',
  'selectLines',
  'scrollLines',
  'scrollPages',
  'scrollToTop',
  'scrollToBottom',
  'scrollToLine',
  'handleFontChange',
] as const;

type Proto = Record<string, unknown>;

export interface GhosttyClasses {
  Terminal?: { prototype: object };
  SelectionManager?: { prototype: object };
  CanvasRenderer?: { prototype: object };
}

const PATCHED = Symbol.for('overseer.onDemandRendering');

export function canPatch(mod: GhosttyClasses): boolean {
  const term = mod.Terminal?.prototype as Proto | undefined;
  const sel = mod.SelectionManager?.prototype as Proto | undefined;
  const ren = mod.CanvasRenderer?.prototype as Proto | undefined;
  const fns = (p: Proto | undefined, names: string[]) => !!p && names.every((n) => typeof p[n] === 'function');
  return (
    fns(term, ['startRenderLoop', 'dispose', 'writeInternal']) &&
    fns(sel, ['requestRender']) &&
    fns(ren, ['render', 'stopCursorBlink', 'setHoveredHyperlinkId', 'setHoveredLinkRange'])
  );
}

export function installOnDemandRendering(mod: GhosttyClasses, sched: FrameScheduler = scheduler): boolean {
  if (!canPatch(mod)) {
    console.warn('ghostty-web internals changed: keeping its built-in render loop');
    return false;
  }
  const term = mod.Terminal!.prototype as Proto & { [PATCHED]?: boolean };
  if (term[PATCHED]) return true;
  term[PATCHED] = true;
  term.startRenderLoop = function (this: PaintTarget) {
    attach(this, sched);
  };
  for (const name of TRIGGERS) {
    const orig = term[name];
    if (typeof orig !== 'function') continue;
    term[name] = function (this: PaintTarget, ...args: unknown[]) {
      const out = (orig as (...a: unknown[]) => unknown).apply(this, args);
      sched.invalidate(this);
      return out;
    };
  }
  const dispose = term.dispose as (...a: unknown[]) => unknown;
  term.dispose = function (this: PaintTarget, ...args: unknown[]) {
    sched.remove(this);
    return dispose.apply(this, args);
  };
  const sel = mod.SelectionManager!.prototype as Proto;
  sel.requestRender = function (this: { terminal?: PaintTarget }) {
    if (this.terminal) sched.invalidate(this.terminal);
  };
  return true;
}

function attach(t: PaintTarget, sched: FrameScheduler) {
  sched.add(t);
  const r = t.renderer;
  if (r) {
    r.stopCursorBlink();
    for (const m of ['setHoveredHyperlinkId', 'setHoveredLinkRange'] as const) {
      const orig = r[m] as ((v: never) => void) | undefined;
      if (typeof orig !== 'function') continue;
      (r as unknown as Proto)[m] = (v: never) => {
        orig.call(r, v);
        sched.invalidate(t);
      };
    }
  }
  const redraw = () => sched.invalidate(t);
  for (const sub of [t.onScroll, t.onSelectionChange]) {
    if (typeof sub !== 'function') continue;
    const d = sub.call(t, redraw);
    sched.onCleanup(t, () => d.dispose());
  }
  const el = t.element;
  if (!el || typeof document === 'undefined') return;
  const focusIn = () => sched.setFocus(t);
  const focusOut = (e: FocusEvent) => {
    if (!el.contains(e.relatedTarget as Node | null) && sched.focused() === t) sched.setFocus(null);
  };
  const key = () => sched.typed(t);
  el.addEventListener('focusin', focusIn);
  el.addEventListener('focusout', focusOut);
  el.addEventListener('keydown', key, { capture: true, passive: true });
  sched.onCleanup(t, () => {
    el.removeEventListener('focusin', focusIn);
    el.removeEventListener('focusout', focusOut);
    el.removeEventListener('keydown', key, { capture: true });
  });
  if (el.contains(document.activeElement)) sched.setFocus(t);
  if (typeof IntersectionObserver === 'function') {
    const io = new IntersectionObserver((entries) => {
      const last = entries[entries.length - 1];
      if (last) sched.setOnScreen(t, last.isIntersecting);
    });
    io.observe(el);
    sched.onCleanup(t, () => io.disconnect());
  }
}
