import { mockSelected, type DaemonClient } from './client';

/** A client of its own for one more host shown in this window. */
export async function newHostClient(slot: string): Promise<DaemonClient> {
  if (mockSelected()) return new (await import('./mock')).MockDaemon();
  return new (await import('./tauri-client')).TauriDaemon(slot);
}
