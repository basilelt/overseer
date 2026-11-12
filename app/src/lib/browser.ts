import { CMD } from './tauri-commands';
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
    return invoke(CMD.browserOpen, { url, ...b, onNav: ch });
  },
  bounds: (b: Bounds) => invoke(CMD.browserBounds, { ...b }),
  navigate: (url: string) => invoke(CMD.browserNavigate, { url }),
  back: () => invoke(CMD.browserBack),
  forward: () => invoke(CMD.browserForward),
  reload: () => invoke(CMD.browserReload),
  hide: () => invoke(CMD.browserHide),
  show: () => invoke(CMD.browserShow),
  external: (url: string) => invoke(CMD.openExternal, { url }),
};

export const nativePage = {
  open(id: string, url: string, b: Bounds, onNav: (e: NavEvent) => void) {
    const ch = new Channel<NavEvent>();
    ch.onmessage = onNav;
    return invoke(CMD.browserOpen, { id, url, ...b, onNav: ch });
  },
  show: (id: string, b: Bounds) => invoke(CMD.browserBounds, { id, ...b }).then(() => invoke(CMD.browserShow, { id })),
  bounds: (id: string, b: Bounds) => invoke(CMD.browserBounds, { id, ...b }),
  navigate: (id: string, url: string) => invoke(CMD.browserNavigate, { id, url }),
  back: (id: string) => invoke(CMD.browserBack, { id }),
  forward: (id: string) => invoke(CMD.browserForward, { id }),
  reload: (id: string) => invoke(CMD.browserReload, { id }),
  hide: (id: string) => invoke(CMD.browserHide, { id }),
  close: (id: string) => invoke(CMD.browserClose, { id }),
  reset: () => invoke(CMD.browserPagesReset),
};
