import { Channel, invoke } from '@tauri-apps/api/core';

export interface NavEvent {
  url: string;
  title?: string;
  loading?: boolean;
}

export interface Bounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function normalizeUrl(input: string): string {
  const s = input.trim();
  if (!s) return '';
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(s) || /^(about|data|file):/i.test(s)) return s;
  if (/^(localhost|127\.0\.0\.1|\[::1\]|0\.0\.0\.0)(:\d+)?(\/|$)/i.test(s)) return `http://${s}`;
  if (/^[^\s/]+:\d+(\/|$)/.test(s) && !s.includes('.')) return `http://${s}`;
  if (!s.includes('.') || /\s/.test(s)) return `https://duckduckgo.com/?q=${encodeURIComponent(s)}`;
  return `https://${s}`;
}

export function boundsOf(el: Element): Bounds {
  const r = el.getBoundingClientRect();
  return { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) };
}

export const nativeBrowser = {
  open(url: string, b: Bounds, onNav: (e: NavEvent) => void) {
    const ch = new Channel<NavEvent>();
    ch.onmessage = onNav;
    return invoke('browser_open', { url, ...b, onNav: ch });
  },
  bounds: (b: Bounds) => invoke('browser_bounds', { ...b }),
  navigate: (url: string) => invoke('browser_navigate', { url }),
  back: () => invoke('browser_back'),
  forward: () => invoke('browser_forward'),
  reload: () => invoke('browser_reload'),
  hide: () => invoke('browser_hide'),
  show: () => invoke('browser_show'),
  external: (url: string) => invoke('open_external', { url }),
};

export const nativePage = {
  open(id: string, url: string, b: Bounds, onNav: (e: NavEvent) => void) {
    const ch = new Channel<NavEvent>();
    ch.onmessage = onNav;
    return invoke('browser_open', { id, url, ...b, onNav: ch });
  },
  show: (id: string, b: Bounds) => invoke('browser_bounds', { id, ...b }).then(() => invoke('browser_show', { id })),
  bounds: (id: string, b: Bounds) => invoke('browser_bounds', { id, ...b }),
  navigate: (id: string, url: string) => invoke('browser_navigate', { id, url }),
  back: (id: string) => invoke('browser_back', { id }),
  forward: (id: string) => invoke('browser_forward', { id }),
  reload: (id: string) => invoke('browser_reload', { id }),
  hide: (id: string) => invoke('browser_hide', { id }),
  close: (id: string) => invoke('browser_close', { id }),
  reset: () => invoke('browser_pages_reset'),
};
