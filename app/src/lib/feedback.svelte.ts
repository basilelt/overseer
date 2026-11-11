import { type Sub, t } from './i18n.svelte';
import { isComment, isFeedbackTerm, UrlWatcher } from './feedback-watch';
import type { Kind } from './feedback';
import type { Recording } from './record';
import type { DaemonClient, TermIO } from './client';
import type { Term } from './types';

export interface Prefill {
  kind?: Kind;
  title?: string;
  description?: string;
  crash?: { report: string | null; appLog: string[]; summary: string };
  recording?: Recording;
}

export interface Result {
  url: string;
  comment: boolean;
}

const WATCH_MS = 60 * 60 * 1000;
const REPORTED_KEY = 'sb.feedback.reported';
const REPORTED_MAX = 100;

function loadReported(): string[] {
  try {
    const v = JSON.parse(globalThis.localStorage?.getItem(REPORTED_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

export function wasReported(termId: string): boolean {
  return loadReported().includes(termId);
}

function markReported(termId: string) {
  try {
    const ids = loadReported().filter((i) => i !== termId);
    ids.push(termId);
    globalThis.localStorage?.setItem(REPORTED_KEY, JSON.stringify(ids.slice(-REPORTED_MAX)));
  } catch {}
}

type Notify = (text: string, kind?: 'info' | 'error', action?: { label: string; run: () => void }) => void;

class Feedback {
  open = $state(false);
  prefill = $state.raw<Prefill | null>(null);
  result = $state.raw<Result | null>(null);
  watching = $state.raw<string[]>([]);
  private stops = new Map<string, () => void>();

  show(prefill: Prefill | null = null) {
    this.prefill = prefill;
    this.result = null;
    this.open = true;
  }

  showResult(r: Result) {
    this.prefill = null;
    this.result = r;
    this.open = true;
  }

  close() {
    this.open = false;
    this.prefill = null;
  }

  announce(r: Result, notify: Notify) {
    this.result = r;
    notify(ft(r.comment ? 'commented' : 'created'), 'info', { label: ft('view'), run: () => this.showResult(r) });
  }

  resume(client: DaemonClient, terms: Term[], notify: Notify) {
    const reported = new Set(loadReported());
    for (const t of terms) {
      if (!isFeedbackTerm(t) || this.stops.has(t.id) || reported.has(t.id)) continue;
      void this.watch(client, t.id, (r) => this.announce(r, notify));
    }
  }

  async watch(client: DaemonClient, termId: string, onFound: (r: Result) => void) {
    if (this.stops.has(termId)) return;
    const w = new UrlWatcher();
    let io: TermIO | null = null;
    let done = false;
    const stop = () => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      io?.close();
      this.stops.delete(termId);
      this.watching = this.watching.filter((t) => t !== termId);
    };
    const timer = setTimeout(stop, WATCH_MS);
    this.stops.set(termId, stop);
    this.watching = [...this.watching, termId];
    try {
      io = await client.attachTerm(
        termId,
        (bytes) => {
          if (done) return;
          const url = w.push(bytes);
          if (!url) return;
          stop();
          if (wasReported(termId)) return;
          markReported(termId);
          onFound({ url, comment: isComment(url) });
        },
        stop,
      );
      if (done) io.close();
    } catch {
      stop();
    }
  }

  stopAll() {
    for (const s of [...this.stops.values()]) s();
  }
}

export const feedback = new Feedback();

type FKey = Sub<'feedback'>;

export function ft(key: FKey, params?: Record<string, string | number>): string {
  return t(`feedback.${key}`, params);
}
