import type { ConnState } from '../types';

export type ServerBannerKey = 'server.update' | 'server.updateOne' | 'server.updateIdle';

export interface ServerBanner {
  key: ServerBannerKey;
  id: string;
  version: string;
  running: string;
  n: number;
  reason: string;
}

export function serverBanner(conn: ConnState, dismissed: string | null = null): ServerBanner | null {
  const u = conn.server;
  if (conn.phase !== 'connected' || !u) return null;
  const id = `${conn.target.kind}:${conn.target.host ?? ''}:${u.running}:${u.installed}`;
  if (dismissed === id) return null;
  const key: ServerBannerKey = u.terms === 0 ? 'server.updateIdle' : u.terms === 1 ? 'server.updateOne' : 'server.update';
  return { key, id, version: u.installed, running: u.running, n: u.terms, reason: u.reason };
}
