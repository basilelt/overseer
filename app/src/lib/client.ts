import type { ConnState, DaemonEvent, DeviceKey, Forward, HostTarget, LocalNetworkReport, PlatformInfo, PortSettings, PortsState, SavedHost, SshHost, UploadProgress, UploadResult } from './types';
import { isDemo } from './demo';

export interface TermIO {
  write(data: string): void;
  resize(cols: number, rows: number): void;
  background?(on: boolean): void;
  close(): void;
}

export interface ConnectOptions {
  serverBinDir?: string;
}

export interface DaemonClient {
  readonly kind: 'tauri' | 'mock';
  request<T = unknown>(method: string, path: string, body?: unknown): Promise<T>;
  fetchBytes(path: string): Promise<ArrayBuffer>;
  subscribe(onEvent: (e: DaemonEvent) => void): () => void;
  attachTerm(termId: string, onData: (bytes: Uint8Array) => void, onClose?: () => void): Promise<TermIO>;
  connect(target: HostTarget, onState: (s: ConnState) => void, opts?: ConnectOptions): Promise<void>;
  disconnect(): Promise<void>;
  sshHosts(): Promise<SshHost[]>;
  watchPorts(onState: (s: PortsState) => void): () => void;
  configurePorts(s: PortSettings): Promise<void>;
  forwardPort(remotePort: number, localPort?: number | null): Promise<Forward>;
  unforwardPort(remotePort: number): Promise<void>;
  answerPrompt(id: number, answer: string | null): Promise<void>;
  localNetworkStatus(host?: string | null): Promise<LocalNetworkReport>;
  openLocalNetworkSettings(): Promise<void>;
  platform(): Promise<PlatformInfo>;
  wake(): Promise<void>;
  restartServer(): Promise<void>;
  deviceKey(): Promise<DeviceKey>;
  savedHosts(): Promise<SavedHost[]>;
  saveHost(host: SavedHost): Promise<SavedHost[]>;
  deleteHost(id: string): Promise<SavedHost[]>;
  forgetHostKey(id: string): Promise<void>;
  copyText(text: string): Promise<void>;
  pasteText(): Promise<string>;
  uploadFile(path: string, onProgress?: (p: UploadProgress) => void): Promise<UploadResult>;
  savePaste(name: string, bytes: Uint8Array): Promise<string>;
  saveDownload(name: string, bytes: Uint8Array): Promise<string>;
}

export class DaemonError extends Error {
  constructor(
    public status: number,
    message: string,
    public body?: unknown,
  ) {
    super(message);
  }
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

let current: DaemonClient | undefined;

async function demoClient(): Promise<DaemonClient> {
  const { MockDaemon } = await import('./mock');
  if (!isTauri()) return new MockDaemon();
  const { invoke } = await import('@tauri-apps/api/core');
  const platform = await invoke<PlatformInfo>('app_platform').catch(() => undefined);
  const m = new MockDaemon({ platform });
  m.copyText = async (text) => void (await invoke('plugin:clipboard-manager|write_text', { text }));
  m.pasteText = () => invoke<string>('plugin:clipboard-manager|read_text');
  m.saveDownload = (name, bytes) => invoke<string>('save_download', bytes, { headers: { 'x-name': encodeURIComponent(name) } });
  return m;
}

export async function getClient(): Promise<DaemonClient> {
  if (current) return current;
  if (isDemo()) return (current = await demoClient());
  const mock = import.meta.env.VITE_MOCK === '1' || !isTauri();
  current = mock ? new (await import('./mock')).MockDaemon() : new (await import('./tauri-client')).TauriDaemon();
  return current;
}

export function setClient(c: DaemonClient) {
  current = c;
}
