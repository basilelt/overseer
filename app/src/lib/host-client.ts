import { isTauri, type DaemonClient } from './client';
import { isDemo } from './demo';

/** A client of its own for one more host shown in this window. */
export async function newHostClient(slot: string): Promise<DaemonClient> {
  if (isDemo() || import.meta.env.VITE_MOCK === '1' || !isTauri()) return new (await import('./mock')).MockDaemon();
  return new (await import('./tauri-client')).TauriDaemon(slot);
}
