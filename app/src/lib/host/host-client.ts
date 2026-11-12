import { mockSelected, type DaemonClient } from '../daemon/client';

/** A client of its own for one more host shown in this window. */
export async function newHostClient(slot: string): Promise<DaemonClient> {
  if (mockSelected()) return new (await import('../daemon/mock')).MockDaemon();
  return new (await import('../daemon/tauri-client')).TauriDaemon(slot);
}
