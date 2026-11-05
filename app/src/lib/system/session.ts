import type { ConnState } from '../types';

export function systemSession(connected: boolean, conn: ConnState, hasClient: boolean): string | null {
  if (!connected || conn.phase !== 'connected' || !hasClient) return null;
  return `${conn.target.kind}:${conn.target.host ?? ''}`;
}
