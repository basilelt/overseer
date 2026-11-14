import { mockSelected, type DaemonClient } from '../daemon/client';

export async function newHostClient(slot: string): Promise<DaemonClient> {
  if (mockSelected()) return new (await import('../daemon/mock')).MockDaemon();
  return new (await import('../daemon/tauri-client')).TauriDaemon(slot);
}
