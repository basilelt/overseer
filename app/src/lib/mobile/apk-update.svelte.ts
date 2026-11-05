import { isTauri } from '../client';
import { type Sub, t } from '../i18n.svelte';

export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
export const RESUME_CHECK_MS = 15 * 60 * 1000;

export type ApkErrorKind = 'network' | 'checksum' | 'signature' | 'failed';

export interface ApkError {
  kind: ApkErrorKind;
  message?: string;
}

export interface ApkStatus {
  current: string;
  supported: boolean;
}

export interface ApkInfo {
  current: string;
  version: string;
  notes: string;
  date: string | null;
  releaseUrl: string;
  size: number;
  ready: boolean;
}

export type ApkCheck = { status: 'none'; current: string } | ({ status: 'available' } & ApkInfo);

export type DownloadEvent = { event: 'progress'; downloaded: number; total: number | null } | { event: 'verifying' };

export type InstallOutcome = 'started' | 'permission' | 'signature';

export interface ApkBackend {
  status(): Promise<ApkStatus>;
  check(): Promise<ApkCheck>;
  download(onProgress: (e: DownloadEvent) => void): Promise<void>;
  install(): Promise<InstallOutcome>;
  allowInstall(): Promise<void>;
  openUrl(url: string): Promise<void>;
}

export type ApkPhase = 'idle' | 'checking' | 'current' | 'available' | 'downloading' | 'verifying' | 'permission' | 'installing' | 'error';

const BUSY: ApkPhase[] = ['checking', 'downloading', 'verifying'];
const KINDS: ApkErrorKind[] = ['network', 'checksum', 'signature', 'failed'];

export class ApkUpdater {
  phase = $state<ApkPhase>('idle');
  info = $state<ApkInfo | null>(null);
  current = $state('');
  error = $state<ApkError | null>(null);
  downloaded = $state(0);
  total = $state<number | null>(null);
  sheet = $state(false);
  backend = $state.raw<ApkBackend | null>(null);
  updates = $state(true);
  private lastCheck = 0;

  constructor(
    backend: ApkBackend | null,
    private readonly now: () => number = Date.now,
  ) {
    this.backend = backend;
  }

  get enabled(): boolean {
    return this.backend !== null;
  }

  get busy(): boolean {
    return BUSY.includes(this.phase);
  }

  get progress(): number | null {
    return this.total ? Math.min(1, this.downloaded / this.total) : null;
  }

  async refreshStatus(): Promise<boolean | null> {
    if (!this.backend) return null;
    try {
      const s = await this.backend.status();
      this.current = s.current;
      if (!s.supported) this.updates = false;
      return s.supported;
    } catch {
      return null;
    }
  }

  async check(manual = false) {
    if (!this.backend || !this.updates) return;
    if (manual) this.sheet = true;
    if (this.busy || this.phase === 'permission' || this.phase === 'installing') return;
    this.phase = 'checking';
    this.error = null;
    try {
      const r = await this.backend.check();
      this.lastCheck = this.now();
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
      this.error = apkError(e);
      this.phase = !manual && this.info ? 'available' : 'error';
    }
  }

  async install() {
    if (!this.backend || !this.info || this.busy) return;
    this.error = null;
    if (!this.info.ready) {
      this.phase = 'downloading';
      this.downloaded = 0;
      this.total = this.info.size || null;
      try {
        await this.backend.download((e) => {
          if (e.event === 'verifying') this.phase = 'verifying';
          else {
            this.downloaded = e.downloaded;
            this.total = e.total ?? this.total;
          }
        });
      } catch (e) {
        this.error = apkError(e);
        this.phase = 'error';
        return;
      }
      this.info = { ...this.info, ready: true };
    }
    await this.handOff();
  }

  private async handOff() {
    if (!this.backend) return;
    try {
      const outcome = await this.backend.install();
      if (outcome === 'permission') this.phase = 'permission';
      else if (outcome === 'signature') this.fail({ kind: 'signature' });
      else this.phase = 'installing';
    } catch (e) {
      this.fail(apkError(e));
    }
  }

  private fail(err: ApkError) {
    this.error = err;
    this.phase = 'error';
  }

  async allowInstall() {
    if (!this.backend) return;
    try {
      await this.backend.allowInstall();
    } catch (e) {
      this.fail(apkError(e));
    }
  }

  resume() {
    if (!this.backend || !this.updates) return;
    if (this.phase === 'permission') return void this.handOff();
    if (this.phase === 'installing') {
      this.phase = 'available';
      return;
    }
    if (this.now() - this.lastCheck >= RESUME_CHECK_MS) void this.check();
  }

  start(intervalMs = CHECK_INTERVAL_MS): () => void {
    if (!this.backend) return () => {};
    if (!this.updates) {
      void this.refreshStatus();
      return () => {};
    }
    this.updates = false;
    let stopped = false;
    let id: ReturnType<typeof setInterval> | undefined;
    void this.refreshStatus().then((supported) => {
      if (stopped || supported === false) return;
      this.updates = true;
      void this.check();
      id = setInterval(() => void this.check(), intervalMs);
    });
    return () => {
      stopped = true;
      clearInterval(id);
    };
  }
}

export function apkError(e: unknown): ApkError {
  const o = (e && typeof e === 'object' ? e : {}) as Record<string, unknown>;
  if (typeof o.kind === 'string' && (KINDS as string[]).includes(o.kind)) {
    return typeof o.message === 'string' ? { kind: o.kind as ApkErrorKind, message: o.message } : { kind: o.kind as ApkErrorKind };
  }
  return { kind: 'failed', message: String((e as Error)?.message ?? e) };
}

export function errorText(err: ApkError): string {
  switch (err.kind) {
    case 'network':
      return at('err.network', { message: err.message ?? '' });
    case 'failed':
      return err.message || at('err.failed');
    default:
      return at(`err.${err.kind}` as AKey);
  }
}

export function pillLabel(u: Pick<ApkUpdater, 'phase' | 'info' | 'progress'>): string | null {
  switch (u.phase) {
    case 'available':
    case 'error':
      return u.info ? at('pill.available', { version: u.info.version }) : null;
    case 'downloading':
      return u.progress === null ? at('pill.downloading') : at('pill.progress', { pct: Math.round(u.progress * 100) });
    case 'verifying':
      return at('pill.verifying');
    case 'permission':
      return at('pill.permission');
    case 'installing':
      return at('pill.installing');
    default:
      return null;
  }
}

export type SheetAction = 'install' | 'allow' | 'reinstall' | 'retry' | null;

export function sheetAction(u: Pick<ApkUpdater, 'phase' | 'info' | 'error'>): SheetAction {
  if (u.phase === 'permission') return 'allow';
  if (u.phase === 'installing') return 'reinstall';
  if (u.phase === 'error' && u.error?.kind === 'signature') return null;
  if (u.info && (u.phase === 'available' || u.phase === 'error')) return 'install';
  if (u.phase === 'error') return 'retry';
  return null;
}

export function formatSize(bytes: number): string {
  if (!bytes) return '';
  return bytes >= 1024 * 1024 ? `${(bytes / (1024 * 1024)).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

type AKey = Sub<'apk'>;

export function at(key: AKey, params?: Record<string, string | number>): string {
  return t(`apk.${key}`, params);
}

async function tauriBackend(): Promise<ApkBackend> {
  const { invoke, Channel } = await import('@tauri-apps/api/core');
  return {
    status: () => invoke<ApkStatus>('apk_update_status'),
    check: () => invoke<ApkCheck>('apk_update_check'),
    download: (onProgress) => {
      const channel = new Channel<DownloadEvent>();
      channel.onmessage = onProgress;
      return invoke<void>('apk_update_download', { onProgress: channel });
    },
    install: () => invoke<InstallOutcome>('apk_update_install'),
    allowInstall: () => invoke<void>('apk_update_allow_install'),
    openUrl: (url) => invoke<void>('open_external', { url }),
  };
}

const lazy: ApkBackend = {
  status: async () => (await tauriBackend()).status(),
  check: async () => (await tauriBackend()).check(),
  download: async (p) => (await tauriBackend()).download(p),
  install: async () => (await tauriBackend()).install(),
  allowInstall: async () => (await tauriBackend()).allowInstall(),
  openUrl: async (u) => (await tauriBackend()).openUrl(u),
};

export const apk = /* @__PURE__ */ new ApkUpdater(null);

export function startApkUpdater(os: string, onResume: (fn: () => void) => () => void): () => void {
  if ((os !== 'android' && os !== 'ios') || !isTauri()) return () => {};
  apk.updates = os === 'android';
  apk.backend = lazy;
  const stop = apk.start();
  const off = onResume(() => apk.resume());
  return () => {
    stop();
    off();
  };
}
