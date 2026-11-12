import * as CMD from './daemon/tauri-commands';
import { isTauri } from './daemon/client';
import type { HostTarget } from './types';

function asTarget(v: unknown): HostTarget | null {
  if (!v || typeof v !== 'object') return null;
  const t = v as Partial<HostTarget>;
  if (t.kind === 'local') return { kind: 'local', label: typeof t.label === 'string' ? t.label : 'Local' };
  if (t.kind === 'ssh' && typeof t.host === 'string' && t.host) return { kind: 'ssh', host: t.host, label: typeof t.label === 'string' && t.label ? t.label : t.host };
  return null;
}

export async function takeHandoff(search: string = globalThis.location?.search ?? ''): Promise<HostTarget | null> {
  if (isTauri()) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return asTarget(await invoke(CMD.windowHandoff));
    } catch {
      return null;
    }
  }
  try {
    return asTarget(JSON.parse(new URLSearchParams(search).get('connect') ?? 'null'));
  } catch {
    return null;
  }
}
