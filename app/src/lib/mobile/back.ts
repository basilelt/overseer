import type { MobileTab } from './layout';

type Handler = () => void;

const handlers: Handler[] = [];

export function onBack(fn: Handler): () => void {
  handlers.push(fn);
  return () => {
    const i = handlers.lastIndexOf(fn);
    if (i >= 0) handlers.splice(i, 1);
  };
}

export function closeTop(): boolean {
  const top = handlers[handlers.length - 1];
  if (!top) return false;
  top();
  return true;
}

let runner: (() => void) | null = null;

export function setBackRunner(fn: () => void): () => void {
  runner = fn;
  return () => {
    if (runner === fn) runner = null;
  };
}

export function runBack(): boolean {
  if (!runner) return false;
  runner();
  return true;
}

export function backHandlers(): number {
  return handlers.length;
}

export interface BackState {
  handlers: number;
  tour: boolean;
  menu: boolean;
  diag: boolean;
  overlay: boolean;
  entry: boolean;
  panel: boolean;
  drawer: boolean;
  subscreen: boolean;
  previousTab: MobileTab | null;
}

export type BackAction = 'handler' | 'tour' | 'menu' | 'diag' | 'overlay' | 'panel' | 'drawer' | 'subscreen' | 'tab' | 'minimize';

export function backAction(s: BackState): BackAction {
  if (s.tour) return 'tour';
  if (s.menu) return 'menu';
  if (s.handlers > 0) return 'handler';
  if (s.diag) return 'diag';
  if (s.entry) return 'minimize';
  if (s.overlay) return 'overlay';
  if (s.panel) return 'panel';
  if (s.drawer) return 'drawer';
  if (s.subscreen) return 'subscreen';
  if (s.previousTab) return 'tab';
  return 'minimize';
}

export function pushTab(history: MobileTab[], from: MobileTab, to: MobileTab, max = 8): MobileTab[] {
  if (from === to) return history;
  const next = history.filter((t) => t !== to && t !== from);
  next.push(from);
  return next.slice(-max);
}

export function escapeCloses(e: { key: string; defaultPrevented: boolean }, open: boolean): boolean {
  return open && e.key === 'Escape' && !e.defaultPrevented;
}
