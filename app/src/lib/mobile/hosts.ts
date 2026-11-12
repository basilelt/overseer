import type { ConnPhase, Endpoint, SavedHost } from '../types';

export interface HostForm {
  id: string;
  label: string;
  host: string;
  port: string;
  user: string;
  auth: 'key' | 'password';
  rememberPassword: boolean;
  jump: string;
}

export type FormError = 'host' | 'user' | 'port' | 'jump';

export function emptyForm(): HostForm {
  return { id: '', label: '', host: '', port: '22', user: '', auth: 'key', rememberPassword: false, jump: '' };
}

export function parseDest(s: string): Endpoint | null {
  const v = s.trim();
  const at = v.indexOf('@');
  const user = at >= 0 ? v.slice(0, at) : '';
  const rest = at >= 0 ? v.slice(at + 1) : v;
  let host = rest;
  let port = 22;
  const colon = rest.lastIndexOf(':');
  if (colon >= 0) {
    const h = rest.slice(0, colon);
    if (!h.includes(':') || h.startsWith('[')) {
      const p = rest.slice(colon + 1);
      if (!/^\d+$/.test(p)) return null;
      port = Number(p);
      host = h;
    }
  }
  host = host.replace(/^\[|\]$/g, '');
  if (!host || /\s/.test(host) || /\s/.test(user) || port < 1 || port > 65535) return null;
  return { host, port, user };
}

export function formatDest(e: Endpoint): string {
  const host = e.host.includes(':') ? `[${e.host}]` : e.host;
  return `${e.user ? `${e.user}@` : ''}${host}${e.port && e.port !== 22 ? `:${e.port}` : ''}`;
}

export function describeHost(h: SavedHost): string {
  const main = formatDest(h);
  return h.jump ? `${main} via ${formatDest(h.jump)}` : main;
}

export function formFromHost(h: SavedHost): HostForm {
  return {
    id: h.id,
    label: h.label,
    host: h.host,
    port: String(h.port),
    user: h.user,
    auth: h.auth,
    rememberPassword: h.rememberPassword,
    jump: h.jump ? formatDest(h.jump) : '',
  };
}

export function hostFromForm(f: HostForm): { host: SavedHost } | { error: FormError } {
  const host = f.host.trim();
  const user = f.user.trim();
  if (!host || /\s/.test(host) || host.includes('@')) return { error: 'host' };
  if (!user || /\s/.test(user) || user.includes('@')) return { error: 'user' };
  const port = Number(f.port.trim() || '22');
  if (!Number.isInteger(port) || port < 1 || port > 65535) return { error: 'port' };
  let jump: Endpoint | null = null;
  if (f.jump.trim()) {
    jump = parseDest(f.jump);
    if (!jump) return { error: 'jump' };
    if (!jump.user) jump.user = user;
  }
  return {
    host: {
      id: f.id,
      label: f.label.trim() || host,
      host,
      port,
      user,
      jump,
      auth: f.auth,
      rememberPassword: f.auth === 'password' && f.rememberPassword,
    },
  };
}

export type ConnectHint = 'notInstalled' | 'deviceKey' | 'hostKeyChanged' | 'unreachable' | null;

export function connectHint(error: string | undefined): ConnectHint {
  if (!error) return null;
  if (error.includes('not installed on this host')) return 'notInstalled';
  if (error.includes("did not accept this device's key")) return 'deviceKey';
  if (error.includes('HOST IDENTIFICATION HAS CHANGED')) return 'hostKeyChanged';
  if (/timed out|No route to host|Network is unreachable|Connection refused|failed to lookup|nodename nor servname/i.test(error)) return 'unreachable';
  return null;
}

export function phoneHostDot(phase: ConnPhase): string {
  return phase === 'connected' ? 'var(--st-input)' : phase === 'error' ? 'var(--st-failed)' : 'var(--st-held)';
}
