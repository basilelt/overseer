import { keyboardInset, layoutFor, mobileUserAgent, phoneParam, type Layout, type MobileTab } from './layout';
import { userAgent } from '../platform';

export const ui = $state<{ layout: Layout; mobileOs: boolean; forcePhone: boolean; drawer: boolean; tab: MobileTab; panel: boolean }>({
  layout: 'desktop',
  mobileOs: false,
  forcePhone: false,
  drawer: false,
  tab: 'agents',
  panel: false,
});

export function isPhone(): boolean {
  return ui.layout === 'phone';
}

function measure() {
  const width = globalThis.innerWidth ?? 0;
  ui.layout = layoutFor({ width, forcePhone: ui.forcePhone, mobileOs: ui.mobileOs });
  globalThis.document?.documentElement.classList.toggle('phone', ui.layout === 'phone');
}

function trackKeyboard(): () => void {
  const vv = globalThis.visualViewport;
  const root = globalThis.document?.documentElement;
  if (!vv || !root) return () => {};
  let frame = 0;
  let last = -1;
  const apply = () => {
    frame = 0;
    const kb = document.activeElement?.matches('input,textarea') ? keyboardInset(globalThis.innerHeight, vv.height, vv.scale || 1) : 0;
    if (kb === last) return;
    last = kb;
    root.style.setProperty('--kb', `${kb}px`);
    root.classList.toggle('kb', kb > 0);
    if (kb > 0 && vv.offsetTop > 0) globalThis.scrollTo?.(0, 0);
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(apply);
  };
  apply();
  vv.addEventListener('resize', schedule);
  addEventListener('resize', schedule);
  const stopResume = onResume(schedule);
  return () => {
    vv.removeEventListener('resize', schedule);
    removeEventListener('resize', schedule);
    stopResume();
    if (frame) cancelAnimationFrame(frame);
    root.style.removeProperty('--kb');
    root.classList.remove('kb');
  };
}

export function startLayout(mobileOs = false): () => void {
  ui.forcePhone = typeof location !== 'undefined' && phoneParam(location.search);
  ui.mobileOs = mobileOs || mobileUserAgent(userAgent());
  measure();
  addEventListener('resize', measure);
  const stopKeyboard = trackKeyboard();
  return () => {
    removeEventListener('resize', measure);
    stopKeyboard();
  };
}

export function onResume(fn: () => void): () => void {
  if (typeof document === 'undefined') return () => {};
  const visible = () => {
    if (document.visibilityState === 'visible') fn();
  };
  document.addEventListener('visibilitychange', visible);
  addEventListener('online', fn);
  return () => {
    document.removeEventListener('visibilitychange', visible);
    removeEventListener('online', fn);
  };
}
