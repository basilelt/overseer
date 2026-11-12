import * as CMD from './daemon/tauri-commands';
import { isTauri } from './daemon/client';
import type { HostTarget } from './types';

export const MAIN_WINDOW = 'main';

interface TauriInternals {
  metadata?: { currentWindow?: { label?: string } };
}

export function windowLabel(search: string = globalThis.location?.search ?? '', internals: TauriInternals | undefined = (globalThis as { __TAURI_INTERNALS__?: TauriInternals }).__TAURI_INTERNALS__): string {
  return internals?.metadata?.currentWindow?.label || new URLSearchParams(search).get('window') || MAIN_WINDOW;
}

export function isMainWindow(): boolean {
  return windowLabel() === MAIN_WINDOW;
}

export function hostKey(t: Pick<HostTarget, 'kind' | 'host'>): string {
  return t.kind === 'local' ? 'local' : `${t.kind}:${t.host ?? ''}`;
}

let opened = 1;

export async function openWindow(target?: HostTarget): Promise<void> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    await (target ? invoke(CMD.windowNew, { connect: target }) : invoke(CMD.windowNew));
    return;
  }
  const url = new URL(globalThis.location.href);
  url.searchParams.set('window', `w-${++opened}`);
  if (target) url.searchParams.set('connect', JSON.stringify(target));
  else url.searchParams.delete('connect');
  globalThis.open?.(url.toString(), '_blank');
}
