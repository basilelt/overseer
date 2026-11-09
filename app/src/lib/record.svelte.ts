import { type Sub, t } from './i18n.svelte';
import { describe, FRAME_RATE, isField, LOG, MAX_EVENTS, MAX_MS, pickMime, shortcut, VIDEO_BPS, type EventKind, type RecEvent, type Recording } from './record';

import { rec, setupRecording, tap, type Phase } from './record-state.svelte';

export type { Phase };

export const SKIP = '[data-rec-skip]';

interface StartOpts {
  video: boolean;
  onDone: (r: Recording) => void;
}

class Recorder {
  get phase(): Phase {
    return rec.phase;
  }

  private set phase(p: Phase) {
    rec.phase = p;
  }

  elapsed = $state(0);
  count = $state(0);
  private events: RecEvent[] = [];
  private t0 = 0;
  private stops: (() => void)[] = [];
  private media: MediaRecorder | null = null;
  private stream: MediaStream | null = null;
  private chunks: Blob[] = [];
  private mime = '';
  private onDone: ((r: Recording) => void) | null = null;
  private typedIn: Element | null = null;
  private finishing = false;

  get on() {
    return this.phase === 'recording';
  }

  setup() {
    setupRecording();
  }

  dismiss() {
    if (this.phase === 'setup') this.phase = 'idle';
  }

  note(kind: EventKind, text: string) {
    if (this.phase !== 'recording' || this.events.length >= MAX_EVENTS) return;
    const last = this.events[this.events.length - 1];
    if (last && last.kind === kind && last.text === text && kind !== 'click') return;
    this.events.push({ at: Date.now() - this.t0, kind, text });
    this.count = this.events.length;
  }

  async start(opts: StartOpts): Promise<void> {
    if (this.phase !== 'idle' && this.phase !== 'setup') return;
    this.phase = 'starting';
    this.onDone = opts.onDone;
    this.finishing = false;
    if (opts.video) {
      try {
        await this.startVideo();
      } catch (e) {
        this.dropVideo();
        this.phase = 'idle';
        this.onDone = null;
        throw e;
      }
    }
    this.events = [];
    this.count = 0;
    this.elapsed = 0;
    this.typedIn = null;
    this.t0 = Date.now();
    this.phase = 'recording';
    tap.note = (kind, text) => this.note(kind, text);
    this.note('start', this.media ? LOG.startedVideo : LOG.started);
    this.listen();
  }

  private async startVideo() {
    const stream = await navigator.mediaDevices.getDisplayMedia({ video: { frameRate: FRAME_RATE }, audio: false, preferCurrentTab: true, selfBrowserSurface: 'include' } as DisplayMediaStreamOptions);
    this.stream = stream;
    this.mime = pickMime((m) => MediaRecorder.isTypeSupported(m));
    const media = new MediaRecorder(stream, { ...(this.mime ? { mimeType: this.mime } : {}), videoBitsPerSecond: VIDEO_BPS });
    this.mime = media.mimeType || this.mime || 'video/webm';
    this.chunks = [];
    media.ondataavailable = (e) => {
      if (e.data.size) this.chunks.push(e.data);
    };
    for (const track of stream.getVideoTracks()) track.addEventListener('ended', () => void this.stop(), { once: true });
    media.start(1000);
    this.media = media;
  }

  private listen() {
    const onClick = (e: MouseEvent) => {
      const el = e.target instanceof Element ? e.target : null;
      if (!el || el.closest(SKIP)) return;
      const d = describe(el);
      if (d) this.note('click', d);
    };
    const onKey = (e: KeyboardEvent) => {
      const el = e.target instanceof Element ? e.target : null;
      if (el?.closest(SKIP)) return;
      const field = isField(el);
      const s = shortcut(e, field);
      if (s) return this.note('key', s);
      if (field && el && e.key.length === 1 && this.typedIn !== el) {
        this.typedIn = el;
        this.note('type', LOG.typed(describe(el) ?? el.tagName.toLowerCase()));
      }
    };
    const onFocus = (e: FocusEvent) => {
      if (e.target !== this.typedIn) this.typedIn = null;
    };
    const onError = (e: ErrorEvent) => this.note('error', e.message || String(e.error));
    const onReject = (e: PromiseRejectionEvent) => this.note('error', String((e.reason as Error)?.message ?? e.reason));
    window.addEventListener('click', onClick, true);
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('focusin', onFocus, true);
    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onReject);
    const tick = setInterval(() => (this.elapsed = Date.now() - this.t0), 1000);
    const limit = setTimeout(() => void this.stop(), MAX_MS);
    this.stops = [
      () => window.removeEventListener('click', onClick, true),
      () => window.removeEventListener('keydown', onKey, true),
      () => window.removeEventListener('focusin', onFocus, true),
      () => window.removeEventListener('error', onError),
      () => window.removeEventListener('unhandledrejection', onReject),
      () => clearInterval(tick),
      () => clearTimeout(limit),
    ];
  }

  private unlisten() {
    for (const s of this.stops) s();
    this.stops = [];
  }

  private dropVideo() {
    for (const track of this.stream?.getTracks() ?? []) track.stop();
    this.stream = null;
    this.media = null;
    this.chunks = [];
  }

  private async flush(): Promise<Blob | null> {
    const media = this.media;
    if (!media) return null;
    if (media.state !== 'inactive') {
      await new Promise<void>((resolve) => {
        media.addEventListener('stop', () => resolve(), { once: true });
        try {
          media.stop();
        } catch {
          resolve();
        }
      });
    }
    const blob = this.chunks.length ? new Blob(this.chunks, { type: this.mime }) : null;
    this.dropVideo();
    return blob;
  }

  async stop(): Promise<Recording | null> {
    if (this.phase !== 'recording' || this.finishing) return null;
    this.finishing = true;
    const duration = Date.now() - this.t0;
    this.unlisten();
    const mime = this.mime;
    const video = await this.flush();
    const rec: Recording = { events: this.events, duration, video, mime: video ? mime : '' };
    const done = this.onDone;
    this.reset();
    done?.(rec);
    return rec;
  }

  cancel() {
    if (this.phase !== 'recording' || this.finishing) return;
    this.unlisten();
    try {
      if (this.media?.state !== 'inactive') this.media?.stop();
    } catch {}
    this.dropVideo();
    this.reset();
  }

  private reset() {
    this.events = [];
    this.count = 0;
    this.elapsed = 0;
    this.typedIn = null;
    this.onDone = null;
    this.mime = '';
    this.finishing = false;
    tap.note = null;
    this.phase = 'idle';
  }
}

export const recorder = new Recorder();

type RKey = Sub<'record'>;

export function rt(key: RKey, params?: Record<string, string | number>): string {
  return t(`record.${key}`, params);
}
