import { isTauri } from './client';

export interface Presence {
  visible: boolean;
  focused: boolean;
}

const state: Presence = { visible: true, focused: true };
const listeners = new Set<(p: Presence) => void>();
let windowFocused = true;
let idle = false;

export function presence(): Readonly<Presence> {
  return state;
}

export function active(): boolean {
  return state.visible && state.focused;
}

export function onPresence(fn: (p: Presence) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function setPresence(next: Partial<Presence>) {
  const visible = next.visible ?? state.visible;
  const focused = next.focused ?? state.focused;
  if (visible === state.visible && focused === state.focused) return;
  state.visible = visible;
  state.focused = focused;
  if (typeof document !== 'undefined') {
    const root = document.documentElement.classList;
    root.toggle('hidden-window', !visible);
    root.toggle('inactive', !visible || !focused);
  }
  for (const l of [...listeners]) l(state);
}

function setFocus(focused = windowFocused) {
  windowFocused = focused;
  setPresence({ focused: focused && !idle });
}

export function setIdle(on: boolean) {
  idle = on;
  setFocus();
}

export function startPresence(): () => void {
  if (typeof document === 'undefined') return () => {};
  const sync = () => {
    setPresence({ visible: document.visibilityState === 'visible' });
    setFocus(document.hasFocus());
  };
  const focus = () => setFocus(true);
  const blur = () => setFocus(false);
  sync();
  document.addEventListener('visibilitychange', sync);
  window.addEventListener('focus', focus);
  window.addEventListener('blur', blur);
  let unlisten: Promise<() => void> | null = null;
  if (isTauri())
    unlisten = import('@tauri-apps/api/window')
      .then(({ getCurrentWindow }) => getCurrentWindow().onFocusChanged(({ payload }) => setFocus(payload)))
      .catch(() => () => {});
  return () => {
    document.removeEventListener('visibilitychange', sync);
    window.removeEventListener('focus', focus);
    window.removeEventListener('blur', blur);
    unlisten?.then((off) => off()).catch(() => {});
  };
}
