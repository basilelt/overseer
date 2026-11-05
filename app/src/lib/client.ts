import type { ConnState, DaemonEvent, DeviceKey, Forward, HostTarget, LocalNetworkReport, PlatformInfo, PortSettings, PortsState, SavedHost, SshHost, UploadProgress, UploadResult } from './types';

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

export async function getClient(): Promise<DaemonClient> {
  if (current) return current;
  const mock = import.meta.env.VITE_MOCK === '1' || !isTauri();
  current = mock ? new (await import('./mock')).MockDaemon() : new (await import('./tauri-client')).TauriDaemon();
  return current;
}

export function setClient(c: DaemonClient) {
  current = c;
}
