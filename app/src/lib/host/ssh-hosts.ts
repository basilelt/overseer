import type { DaemonClient } from '../daemon/client';
import type { SshHost } from '../types';

export async function loadSshHosts(client: Pick<DaemonClient, 'sshHosts'> | null | undefined, mobile: boolean): Promise<SshHost[]> {
  if (!client || mobile) return [];
  return client.sshHosts().catch(() => []);
}
