import * as CMD from './tauri-commands';
import { Channel, invoke } from '@tauri-apps/api/core';
import { DaemonError, type ConnectOptions, type DaemonClient, type TermIO } from './client';
import type { ConnState, DaemonEvent, DeviceKey, Forward, HostTarget, LocalNetworkReport, PlatformInfo, PortSettings, PortsState, SavedHost, SshHost, UploadProgress, UploadResult } from '../types';

interface RawResponse {
  status: number;
  body: unknown;
}

export class TauriDaemon implements DaemonClient {
  readonly kind = 'tauri' as const;

  /** Extra hosts of a window each get a slot: their own connection beside the window's own. */
  constructor(readonly slot: string | null = null) {}

  private s<T extends object>(args?: T): (T & { slot?: string }) | undefined {
    return this.slot ? ({ ...args, slot: this.slot } as T & { slot?: string }) : args;
  }

  async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await invoke<RawResponse>(CMD.daemonRequest, this.s({ method, path, body: body ?? null }));
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
      await invoke(CMD.daemonBytes, this.s({ path, onData }));
      return await data;
    } catch (e) {
      if (typeof e === 'object' && e && 'error' in e) throw new DaemonError(Number((e as { status?: number }).status ?? 0), String(e.error), e);
      throw new DaemonError(0, String(e));
    }
  }

  subscribe(onEvent: (e: DaemonEvent) => void): () => void {
    const channel = new Channel<DaemonEvent>();
    channel.onmessage = onEvent;
    const id = invoke<number>(CMD.daemonEvents, this.s({ onEvent: channel }));
    return () => {
      id.then((sub) => invoke(CMD.daemonEventsStop, this.s({ sub }))).catch(() => {});
    };
  }

  async attachTerm(termId: string, onData: (bytes: Uint8Array) => void, onClose?: () => void): Promise<TermIO> {
    const channel = new Channel<ArrayBuffer | number[]>();
    channel.onmessage = (m) => {
      if (m instanceof ArrayBuffer) onData(new Uint8Array(m));
      else if (Array.isArray(m) && m.length === 0) onClose?.();
      else onData(Uint8Array.from(m));
    };
    const attach = await invoke<number>(CMD.termAttach, this.s({ termId, onData: channel }));
    return {
      write: (data) => void invoke(CMD.termWrite, this.s({ attach, data })).catch(() => {}),
      resize: (cols, rows) => void invoke(CMD.termResize, this.s({ attach, cols, rows })).catch(() => {}),
      background: (on) => void invoke(CMD.termBackground, this.s({ attach, background: on })).catch(() => {}),
      close: () => void invoke(CMD.termDetach, this.s({ attach })).catch(() => {}),
    };
  }

  async connect(target: HostTarget, onState: (s: ConnState) => void, opts: ConnectOptions = {}): Promise<void> {
    const channel = new Channel<ConnState>();
    channel.onmessage = onState;
    await invoke(CMD.connConnect, this.s({ target, onState: channel, serverBinDir: opts.serverBinDir ?? null }));
  }

  async dropSlots(): Promise<void> {
    await invoke(CMD.connDropSlots);
  }

  async disconnect(): Promise<void> {
    await invoke(CMD.connDisconnect, this.s());
  }

  sshHosts(): Promise<SshHost[]> {
    return invoke<SshHost[]>(CMD.sshHosts);
  }

  watchPorts(onState: (s: PortsState) => void): () => void {
    const channel = new Channel<PortsState>();
    channel.onmessage = onState;
    const id = invoke<number>(CMD.portsWatch, this.s({ onState: channel }));
    return () => {
      id.then((sub) => invoke(CMD.portsWatchStop, this.s({ sub }))).catch(() => {});
    };
  }

  async configurePorts(settings: PortSettings): Promise<void> {
    await invoke(CMD.portsConfigure, this.s({ settings }));
  }

  forwardPort(remotePort: number, localPort: number | null = null): Promise<Forward> {
    return invoke<Forward>(CMD.portsForward, this.s({ remotePort, localPort }));
  }

  async unforwardPort(remotePort: number): Promise<void> {
    await invoke(CMD.portsUnforward, this.s({ remotePort }));
  }

  async answerPrompt(id: number, answer: string | null): Promise<void> {
    await invoke(CMD.connAnswer, this.s({ id, answer }));
  }

  localNetworkStatus(host: string | null = null): Promise<LocalNetworkReport> {
    return invoke<LocalNetworkReport>(CMD.localNetworkStatus, { host });
  }

  async openLocalNetworkSettings(): Promise<void> {
    await invoke(CMD.openLocalNetworkSettings);
  }

  platform(): Promise<PlatformInfo> {
    return invoke<PlatformInfo>(CMD.appPlatform);
  }

  async wake(): Promise<void> {
    await invoke(CMD.connWake, this.s());
  }

  async restartServer(): Promise<void> {
    await invoke(CMD.connRestartServer, this.s());
  }

  deviceKey(): Promise<DeviceKey> {
    return invoke<DeviceKey>(CMD.directDeviceKey);
  }

  savedHosts(): Promise<SavedHost[]> {
    return invoke<SavedHost[]>(CMD.directHosts);
  }

  saveHost(host: SavedHost): Promise<SavedHost[]> {
    return invoke<SavedHost[]>(CMD.directHostSave, { host });
  }

  deleteHost(id: string): Promise<SavedHost[]> {
    return invoke<SavedHost[]>(CMD.directHostDelete, { id });
  }

  async forgetHostKey(id: string): Promise<void> {
    await invoke(CMD.directHostForgetKey, { id });
  }

  async copyText(text: string): Promise<void> {
    await invoke(CMD.clipboardWriteText, { text });
  }

  pasteText(): Promise<string> {
    return invoke<string>(CMD.clipboardReadText);
  }

  uploadFile(path: string, onProgress?: (p: UploadProgress) => void): Promise<UploadResult> {
    const channel = new Channel<UploadProgress>();
    channel.onmessage = (p) => onProgress?.(p);
    return invoke<UploadResult>(CMD.uploadFile, this.s({ path, onProgress: channel }));
  }

  savePaste(name: string, bytes: Uint8Array): Promise<string> {
    return invoke<string>(CMD.savePaste, bytes, { headers: { 'x-name': encodeURIComponent(name) } });
  }

  saveDownload(name: string, bytes: Uint8Array): Promise<string> {
    return invoke<string>(CMD.saveDownload, bytes, { headers: { 'x-name': encodeURIComponent(name) } });
  }
}
