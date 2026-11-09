import { isTauri } from './client';
import { isDemo } from './demo';
import { type Sub, t } from './i18n.svelte';

export type InstallKind = 'bundle' | 'msix';

export interface UpdateInfo {
  current: string;
  version: string;
  notes: string;
  date: string | null;
  kind: InstallKind;
  releaseUrl: string;
}

export type CheckResult = { status: 'none'; current: string } | ({ status: 'available' } & UpdateInfo);

export type ProgressEvent =
  | { event: 'progress'; downloaded: number; total: number | null }
  | { event: 'installing' };

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
  openUrl(url: string): Promise<void>;
}

export type Phase = 'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'installing' | 'updated' | 'restarting' | 'error';

export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const BUSY: Phase[] = ['checking', 'downloading', 'installing', 'restarting'];

export class Updater {
  phase = $state<Phase>('idle');
  info = $state<UpdateInfo | null>(null);
  current = $state('');
  error = $state('');
  downloaded = $state(0);
  total = $state<number | null>(null);
  dialog = $state(false);
  status = $state<UpdateStatus | null>(null);
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
    if (!this.backend || !this.info || this.busy) return;
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
    this.phase = 'updated';
  }

  async restart() {
    if (!this.backend || this.phase !== 'updated' || this.info?.kind === 'msix') return;
    this.phase = 'restarting';
    this.error = '';
    try {
      await this.backend.relaunch();
    } catch (e) {
      this.error = message(e);
      this.phase = 'updated';
    }
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

export function pillLabel(u: Pick<Updater, 'phase' | 'info' | 'progress'>): string | null {
  switch (u.phase) {
    case 'available':
    case 'error':
      return u.info ? ut('pill.available', { version: u.info.version }) : null;
    case 'downloading':
      return u.progress === null ? ut('pill.downloading') : ut('pill.progress', { pct: Math.round(u.progress * 100) });
    case 'installing':
      return ut('pill.installing');
    case 'updated':
      return u.info?.kind === 'msix' ? ut('pill.msix') : ut('pill.updated');
    case 'restarting':
      return ut('pill.restarting');
    default:
      return null;
  }
}

export type DialogAction = 'install' | 'restart' | 'retry' | null;

export function dialogAction(u: Pick<Updater, 'phase' | 'info'>): DialogAction {
  if (u.phase === 'updated') return u.info?.kind === 'msix' ? null : 'restart';
  if (!u.info) return u.phase === 'error' ? 'retry' : null;
  return u.phase === 'available' || u.phase === 'error' ? 'install' : null;
}

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
    openUrl: (url) => invoke<void>('open_external', { url }),
  };
}

const lazy: UpdaterBackend = {
  status: async () => (await tauriBackend()).status(),
  check: async () => (await tauriBackend()).check(),
  install: async (p) => (await tauriBackend()).install(p),
  relaunch: async () => (await tauriBackend()).relaunch(),
  openUrl: async (u) => (await tauriBackend()).openUrl(u),
};

export const updater = new Updater(isTauri() && !isDemo() ? lazy : null);

export function startUpdater(): () => void {
  return import.meta.env.PROD ? updater.start() : (void updater.refreshStatus(), () => {});
}
