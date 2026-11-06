import { isTauri } from './client';
import { isDemo } from './demo';
import { type Sub, t } from './i18n.svelte';

export type InstallKind = 'bundle' | 'homebrew' | 'package';

export interface UpdateInfo {
  current: string;
  version: string;
  notes: string;
  date: string | null;
  kind: InstallKind;
  releaseUrl: string;
}

export type CheckResult = { status: 'none'; current: string } | ({ status: 'available' } & UpdateInfo);

export type BrewStep = 'trust' | 'update' | 'upgrade' | 'formula';

export type ProgressEvent =
  | { event: 'progress'; downloaded: number; total: number | null }
  | { event: 'installing' }
  | { event: 'step'; name: BrewStep }
  | { event: 'line'; text: string };

export type BrewError =
  | { kind: 'missing' }
  | { kind: 'untrusted'; tap: string }
  | { kind: 'stale' }
  | { kind: 'failed'; message: string };

export interface UpdateStatus {
  current: string;
  kind: InstallKind;
  releasesUrl: string;
}

export interface UpdaterBackend {
  status(): Promise<UpdateStatus>;
  check(): Promise<CheckResult>;
  install(onProgress: (e: ProgressEvent) => void): Promise<void>;
  relaunch(): Promise<void>;
  brew(trust: string | null, onProgress: (e: ProgressEvent) => void): Promise<string | null>;
  brewRelaunch(): Promise<void>;
  openUrl(url: string): Promise<void>;
}

export type Phase = 'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'installing' | 'updating' | 'updated' | 'restarting' | 'error';

export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
export const LOG_LINES = 1000;
const BUSY: Phase[] = ['checking', 'downloading', 'installing', 'updating', 'restarting'];

export class Updater {
  phase = $state<Phase>('idle');
  info = $state<UpdateInfo | null>(null);
  current = $state('');
  error = $state('');
  downloaded = $state(0);
  total = $state<number | null>(null);
  dialog = $state(false);
  status = $state<UpdateStatus | null>(null);
  log = $state<string[]>([]);
  step = $state<BrewStep | null>(null);
  brewError = $state<BrewError | null>(null);
  installed = $state<string | null>(null);

  constructor(readonly backend: UpdaterBackend | null) {}

  get enabled(): boolean {
    return this.backend !== null;
  }

  get busy(): boolean {
    return BUSY.includes(this.phase);
  }

  get progress(): number | null {
    return this.total ? Math.min(1, this.downloaded / this.total) : null;
  }

  get installedVersion(): string {
    return this.installed ?? this.info?.version ?? '';
  }

  get lastLine(): string {
    return this.log.at(-1) ?? '';
  }

  async refreshStatus() {
    if (!this.backend) return;
    try {
      this.status = await this.backend.status();
      this.current = this.status.current;
    } catch {}
  }

  async check(manual = false) {
    if (!this.backend || this.busy || this.phase === 'updated') {
      if (manual) this.dialog = true;
      return;
    }
    if (manual) this.dialog = true;
    this.phase = 'checking';
    this.error = '';
    try {
      const r = await this.backend.check();
      if (!r) throw new Error(ut('unavailable'));
      if (r.status === 'available') {
        const { status: _, ...info } = r;
        this.info = info;
        this.current = info.current;
        this.phase = 'available';
      } else {
        this.info = null;
        this.current = r.current;
        this.phase = 'current';
      }
    } catch (e) {
      this.error = message(e);
      this.phase = !manual && this.info ? 'available' : 'error';
    }
  }

  async install() {
    if (!this.backend || !this.info || this.info.kind !== 'bundle' || this.busy) return;
    this.phase = 'downloading';
    this.error = '';
    this.downloaded = 0;
    this.total = null;
    try {
      await this.backend.install((e) => {
        if (e.event === 'installing') this.phase = 'installing';
        else if (e.event === 'progress') {
          this.downloaded = e.downloaded;
          this.total = e.total;
        }
      });
      this.updated(this.info.version);
    } catch (e) {
      this.error = message(e);
      this.phase = 'error';
    }
  }

  private updated(version: string | null) {
    this.installed = version || null;
    this.error = '';
    this.brewError = null;
    this.phase = 'updated';
  }

  async brewUpgrade(trust: string | null = null) {
    if (!this.backend || this.info?.kind !== 'homebrew' || this.busy) return;
    this.phase = 'updating';
    this.error = '';
    this.brewError = null;
    this.step = null;
    this.log = [];
    try {
      const version = await this.backend.brew(trust, (e) => {
        if (e.event === 'step') this.step = e.name;
        else if (e.event === 'line') {
          this.log.push(e.text);
          if (this.log.length > LOG_LINES) this.log.splice(0, this.log.length - LOG_LINES);
        }
      });
      this.updated(version ?? null);
    } catch (e) {
      this.fail(brewError(e));
    }
  }

  async restart() {
    if (!this.backend || this.phase !== 'updated') return;
    this.phase = 'restarting';
    this.error = '';
    try {
      if (this.info?.kind === 'homebrew') await this.backend.brewRelaunch();
      else await this.backend.relaunch();
    } catch (e) {
      this.error = message(e);
      this.phase = 'updated';
    }
  }

  private fail(err: BrewError) {
    this.brewError = err;
    this.error = brewMessage(err);
    this.phase = 'error';
  }

  start(intervalMs = CHECK_INTERVAL_MS): () => void {
    if (!this.backend) return () => {};
    void this.refreshStatus();
    void this.check();
    const id = setInterval(() => void this.check(), intervalMs);
    return () => clearInterval(id);
  }
}

function message(e: unknown): string {
  return String((e as Error)?.message ?? e);
}

export function brewError(e: unknown): BrewError {
  const o = (e && typeof e === 'object' ? e : {}) as Record<string, unknown>;
  switch (o.kind) {
    case 'missing':
    case 'stale':
      return { kind: o.kind as 'missing' | 'stale' };
    case 'untrusted':
      return { kind: 'untrusted', tap: typeof o.tap === 'string' ? o.tap : BREW_TAP };
    case 'failed':
      return { kind: 'failed', message: String(o.message ?? '') };
    default:
      return { kind: 'failed', message: message(e) };
  }
}

export function brewMessage(err: BrewError): string {
  switch (err.kind) {
    case 'missing':
      return ut('err.missing');
    case 'untrusted':
      return ut('err.untrusted', { tap: err.tap });
    case 'stale':
      return ut('err.notInTap');
    case 'failed':
      return err.message;
  }
}

export function pillLabel(u: Pick<Updater, 'phase' | 'info' | 'progress'>): string | null {
  switch (u.phase) {
    case 'available':
    case 'error':
      return u.info ? ut('pill.available', { version: u.info.version }) : null;
    case 'downloading':
      return u.progress === null ? ut('pill.downloading') : ut('pill.progress', { pct: Math.round(u.progress * 100) });
    case 'installing':
      return ut('pill.installing');
    case 'updating':
      return ut('pill.updating');
    case 'updated':
      return ut('pill.updated');
    case 'restarting':
      return ut('pill.restarting');
    default:
      return null;
  }
}

export function brewStepLabel(step: BrewStep | null): string {
  switch (step) {
    case 'trust':
      return ut('step.trust');
    case 'update':
      return ut('step.update');
    case 'upgrade':
      return ut('step.upgrade');
    case 'formula':
      return ut('step.formula');
    default:
      return ut('pill.updating');
  }
}

export type DialogAction = 'install' | 'brew' | 'release' | 'restart' | 'retry' | null;

export function dialogAction(u: Pick<Updater, 'phase' | 'info'>): DialogAction {
  if (u.phase === 'updated') return 'restart';
  if (!u.info) return u.phase === 'error' ? 'retry' : null;
  if (u.info.kind === 'homebrew') return 'brew';
  if (u.info.kind === 'package') return 'release';
  return u.phase === 'available' || u.phase === 'error' ? 'install' : null;
}

export const BREW_TAP = 'basilelt/tap';
export const BREW_UPGRADE = `brew update && brew upgrade --cask ${BREW_TAP}/overseer`;

type UKey = Sub<'updater'>;

export function ut(key: UKey, params?: Record<string, string | number>): string {
  return t(`updater.${key}`, params);
}

async function tauriBackend(): Promise<UpdaterBackend> {
  const { invoke, Channel } = await import('@tauri-apps/api/core');
  return {
    status: () => invoke<UpdateStatus>('update_status'),
    check: () => invoke<CheckResult>('update_check'),
    install: (onProgress) => {
      const channel = new Channel<ProgressEvent>();
      channel.onmessage = onProgress;
      return invoke<void>('update_install', { onProgress: channel });
    },
    relaunch: () => invoke<void>('update_relaunch'),
    brew: (trust, onProgress) => {
      const channel = new Channel<ProgressEvent>();
      channel.onmessage = onProgress;
      return invoke<string | null>('update_brew', { trust, onProgress: channel });
    },
    brewRelaunch: () => invoke<void>('update_brew_relaunch'),
    openUrl: (url) => invoke<void>('open_external', { url }),
  };
}

const lazy: UpdaterBackend = {
  status: async () => (await tauriBackend()).status(),
  check: async () => (await tauriBackend()).check(),
  install: async (p) => (await tauriBackend()).install(p),
  relaunch: async () => (await tauriBackend()).relaunch(),
  brew: async (t, p) => (await tauriBackend()).brew(t, p),
  brewRelaunch: async () => (await tauriBackend()).brewRelaunch(),
  openUrl: async (u) => (await tauriBackend()).openUrl(u),
};

export const updater = new Updater(isTauri() && !isDemo() ? lazy : null);

export function startUpdater(): () => void {
  return import.meta.env.PROD ? updater.start() : (void updater.refreshStatus(), () => {});
}
