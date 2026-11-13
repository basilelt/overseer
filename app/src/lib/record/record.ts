import { comboModifiers } from '../commands/shortcuts';
import { clock, redact } from '../feedback/feedback';
import { isTauri, userAgent } from '../platform';

export { clock };

export type EventKind = 'start' | 'click' | 'key' | 'type' | 'view' | 'error' | 'toast';

export interface RecEvent {
  at: number;
  kind: EventKind;
  text: string;
}

export interface Recording {
  events: RecEvent[];
  duration: number;
  video: Blob | null;
  mime: string;
}

export const MAX_MS = 2 * 60 * 1000;
export const MAX_EVENTS = 500;
export const VIDEO_BPS = 600_000;
export const FRAME_RATE = 15;
const LABEL_MAX = 60;
const MIMES = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm', 'video/mp4'];

function clip(s: string): string {
  const one = s.replace(/\s+/g, ' ').trim();
  return one.length > LABEL_MAX ? `${one.slice(0, LABEL_MAX - 1).trimEnd()}…` : one;
}

export function isField(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  return tag === 'TEXTAREA' || (tag === 'INPUT' && !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file'].includes((el as HTMLInputElement).type)) || (el as HTMLElement).isContentEditable === true || !!el.closest('[data-terminal], canvas');
}

const CONTROL = 'button, a, [role="button"], [role="tab"], [role="radio"], [role="menuitem"], [role="option"], [role="checkbox"], [role="switch"], input, select, textarea, summary';

export function describe(target: Element | null): string | null {
  if (!target) return null;
  if (target.closest('[data-terminal]')) return 'terminal';
  const el = target.closest(CONTROL);
  if (!el) {
    const tour = target.closest('[data-tour]')?.getAttribute('data-tour');
    return tour ? `area "${clip(tour)}"` : null;
  }
  const own = el.getAttribute('aria-label') || el.getAttribute('title') || el.getAttribute('placeholder');
  const field = isField(el);
  const text = own || (field ? '' : ((el.tagName === 'INPUT' ? el.closest('label')?.textContent : el.textContent) ?? '').trim()) || el.getAttribute('data-tour') || '';
  const role = el.getAttribute('role') || el.tagName.toLowerCase();
  const name = clip(redact(text));
  if (!name) return field ? `${role} field` : null;
  return `${role} "${name}"`;
}

export const LOG = {
  started: 'Recording started',
  startedVideo: 'Recording started with screen video',
  typed: (target: string) => `typed in ${target}`,
  view: (view: string) => `view ${view}`,
  overlay: (overlay: string) => `opened ${overlay}`,
};

const NAMED = new Set(['Escape', 'Enter', 'Tab', 'Backspace', 'Delete', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'PageUp', 'PageDown', 'Home', 'End']);

export function shortcut(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>, inField: boolean): string | null {
  if (['Control', 'Meta', 'Alt', 'Shift'].includes(e.key)) return null;
  const mods = comboModifiers({ ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey }, false);
  const fkey = /^F\d{1,2}$/.test(e.key);
  if (inField && !e.ctrlKey && !e.metaKey && !e.altKey && !fkey && e.key !== 'Escape') return null;
  if (!mods.length && !fkey && !NAMED.has(e.key)) return null;
  if (mods.length === 1 && mods[0] === 'Shift' && !fkey && !NAMED.has(e.key)) return null;
  const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  return [...mods, key].join('+');
}

export function formatLog(events: RecEvent[]): string {
  return redact(events.map((e) => `${clock(e.at)}  ${e.kind.padEnd(5)}  ${e.text}`).join('\n'));
}

export function activeAt(events: RecEvent[], ms: number): number {
  let lo = 0;
  let hi = events.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (events[mid].at <= ms) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

export function pickMime(supported: (m: string) => boolean): string {
  return MIMES.find((m) => supported(m)) ?? '';
}

export function extFor(mime: string): string {
  return mime.startsWith('video/mp4') ? 'mp4' : 'webm';
}

const CONSTRAINT_ERRORS = ['OverconstrainedError', 'TypeError', 'NotSupportedError'];
const REFUSAL_ERRORS = ['NotAllowedError', 'AbortError', 'SecurityError'];

export function isConstraintError(e: unknown): boolean {
  const { name = '', message = '' } = (e ?? {}) as { name?: string; message?: string };
  return CONSTRAINT_ERRORS.includes(name) || /constraint/i.test(message);
}

export function isRefusal(e: unknown): boolean {
  return REFUSAL_ERRORS.includes((e as { name?: string } | null)?.name ?? '');
}

type GetDisplay = (options: DisplayMediaStreamOptions) => Promise<MediaStream>;

export async function requestDisplay(get: GetDisplay): Promise<MediaStream> {
  try {
    return await get({ video: { frameRate: FRAME_RATE }, audio: false, preferCurrentTab: true, selfBrowserSurface: 'include' } as DisplayMediaStreamOptions);
  } catch (e) {
    if (!isConstraintError(e)) throw e;
    return get({ video: true });
  }
}

export function webkitGtkShell(tauri: boolean, ua: string): boolean {
  return tauri && /linux/i.test(ua) && !/android/i.test(ua);
}

export function canVideo(g: typeof globalThis = globalThis, shell = webkitGtkShell(isTauri(), userAgent())): boolean {
  if (shell) return false;
  const md = (g.navigator as Navigator | undefined)?.mediaDevices;
  return typeof md?.getDisplayMedia === 'function' && typeof (g as { MediaRecorder?: unknown }).MediaRecorder === 'function';
}

export function fileName(now: Date, mime: string): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `recording-${now.getFullYear()}${p(now.getMonth() + 1)}${p(now.getDate())}-${p(now.getHours())}${p(now.getMinutes())}${p(now.getSeconds())}.${extFor(mime)}`;
}
