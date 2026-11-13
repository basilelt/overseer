import { hostId } from '../hooks';
import type { ConnState, Lifecycle } from '../types';

export function keepRunningOffer(l: Lifecycle | null, conn: ConnState, offered: string[]): boolean {
  if (conn.phase !== 'connected' || conn.target.kind === 'local' || !l) return false;
  return l.autostart.supported && !l.autostart.enabled && !offered.includes(hostId(conn));
}

export function lingerOff(l: Lifecycle | null): boolean {
  return !!l?.autostart.enabled && l.autostart.platform === 'systemd' && l.autostart.linger === false;
}
