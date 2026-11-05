import { Channel, invoke } from '@tauri-apps/api/core';
import { DaemonError, type ConnectOptions, type DaemonClient, type TermIO } from './client';
import type { ConnState, DaemonEvent, DeviceKey, Forward, HostTarget, LocalNetworkReport, PlatformInfo, PortSettings, PortsState, SavedHost, SshHost, UploadProgress, UploadResult } from './types';

interface RawResponse {
  status: number;
  body: unknown;
}

export class TauriDaemon implements DaemonClient {
  readonly kind = 'tauri' as const;

  async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await invoke<RawResponse>('daemon_request', { method, path, body: body ?? null });
    if (res.status >= 400) {
      const msg = typeof res.body === 'object' && res.body && 'error' in res.body ? String(res.body.error) : `HTTP ${res.status}`;
      throw new DaemonError(res.status, msg, res.body);
    }
    return res.body as T;
  }

  async fetchBytes(path: string): Promise<ArrayBuffer> {
    try {
      const onData = new Channel<ArrayBuffer | number[]>();
      const data = new Promise<ArrayBuffer>((resolve) => (onData.onmessage = (m) => resolve(m instanceof ArrayBuffer ? m : Uint8Array.from(m).buffer)));
      await invoke('daemon_bytes', { path, onData });
      return await data;
    } catch (e) {
      if (typeof e === 'object' && e && 'error' in e) throw new DaemonError(Number((e as { status?: number }).status ?? 0), String(e.error), e);
      throw new DaemonError(0, String(e));
    }
  }

  subscribe(onEvent: (e: DaemonEvent) => void): () => void {
    const channel = new Channel<DaemonEvent>();
    channel.onmessage = onEvent;
    const id = invoke<number>('daemon_events', { onEvent: channel });
    return () => {
      id.then((sub) => invoke('daemon_events_stop', { sub })).catch(() => {});
    };
  }

  async attachTerm(termId: string, onData: (bytes: Uint8Array) => void, onClose?: () => void): Promise<TermIO> {
    const channel = new Channel<ArrayBuffer | number[]>();
    channel.onmessage = (m) => {
      if (m instanceof ArrayBuffer) onData(new Uint8Array(m));
      else if (Array.isArray(m) && m.length === 0) onClose?.();
      else onData(Uint8Array.from(m));
    };
    const attach = await invoke<number>('term_attach', { termId, onData: channel });
    return {
      write: (data) => void invoke('term_write', { attach, data }).catch(() => {}),
      resize: (cols, rows) => void invoke('term_resize', { attach, cols, rows }).catch(() => {}),
      background: (on) => void invoke('term_background', { attach, background: on }).catch(() => {}),
      close: () => void invoke('term_detach', { attach }).catch(() => {}),
    };
  }

  async connect(target: HostTarget, onState: (s: ConnState) => void, opts: ConnectOptions = {}): Promise<void> {
    const channel = new Channel<ConnState>();
    channel.onmessage = onState;
    await invoke('conn_connect', { target, onState: channel, serverBinDir: opts.serverBinDir ?? null });
  }

  async disconnect(): Promise<void> {
    await invoke('conn_disconnect');
  }

  sshHosts(): Promise<SshHost[]> {
    return invoke<SshHost[]>('ssh_hosts');
  }

  watchPorts(onState: (s: PortsState) => void): () => void {
    const channel = new Channel<PortsState>();
    channel.onmessage = onState;
    const id = invoke<number>('ports_watch', { onState: channel });
    return () => {
      id.then((sub) => invoke('ports_watch_stop', { sub })).catch(() => {});
    };
  }

  async configurePorts(settings: PortSettings): Promise<void> {
    await invoke('ports_configure', { settings });
  }

  forwardPort(remotePort: number, localPort: number | null = null): Promise<Forward> {
    return invoke<Forward>('ports_forward', { remotePort, localPort });
  }

  async unforwardPort(remotePort: number): Promise<void> {
    await invoke('ports_unforward', { remotePort });
  }

  async answerPrompt(id: number, answer: string | null): Promise<void> {
    await invoke('conn_answer', { id, answer });
  }

  localNetworkStatus(host: string | null = null): Promise<LocalNetworkReport> {
    return invoke<LocalNetworkReport>('local_network_status', { host });
  }

  async openLocalNetworkSettings(): Promise<void> {
    await invoke('open_local_network_settings');
  }

  platform(): Promise<PlatformInfo> {
    return invoke<PlatformInfo>('app_platform');
  }

  async wake(): Promise<void> {
    await invoke('conn_wake');
  }

  async restartServer(): Promise<void> {
    await invoke('conn_restart_server');
  }

  deviceKey(): Promise<DeviceKey> {
    return invoke<DeviceKey>('direct_device_key');
  }

  savedHosts(): Promise<SavedHost[]> {
    return invoke<SavedHost[]>('direct_hosts');
  }

  saveHost(host: SavedHost): Promise<SavedHost[]> {
    return invoke<SavedHost[]>('direct_host_save', { host });
  }

  deleteHost(id: string): Promise<SavedHost[]> {
    return invoke<SavedHost[]>('direct_host_delete', { id });
  }

  async forgetHostKey(id: string): Promise<void> {
    await invoke('direct_host_forget_key', { id });
  }

  async copyText(text: string): Promise<void> {
    await invoke('plugin:clipboard-manager|write_text', { text });
  }

  pasteText(): Promise<string> {
    return invoke<string>('plugin:clipboard-manager|read_text');
  }

  uploadFile(path: string, onProgress?: (p: UploadProgress) => void): Promise<UploadResult> {
    const channel = new Channel<UploadProgress>();
    channel.onmessage = (p) => onProgress?.(p);
    return invoke<UploadResult>('upload_file', { path, onProgress: channel });
  }

  savePaste(name: string, bytes: Uint8Array): Promise<string> {
    return invoke<string>('save_paste', bytes, { headers: { 'x-name': encodeURIComponent(name) } });
  }
}
