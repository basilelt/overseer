import type { Forward, PortSettings, PortsState } from '../types';

export interface PortRow {
  port: number;
  addr?: string;
  local?: string;
  url?: string;
  process?: string;
  pid?: number;
  project?: string;
  termId?: string;
  source?: Forward['source'];
  detected: boolean;
  forwarded: boolean;
  error?: string;
}

export const EMPTY_PORTS: PortsState = { remote: false, detected: [], forwards: [] };

const MAX_RANGE = 1024;

export function localUrl(port: number): string {
  return `http://localhost:${port}`;
}

export function forwardUrl(f: Forward): string {
  return f.url || localUrl(f.localPort);
}

export function portRows(s: PortsState): PortRow[] {
  const rows = new Map<number, PortRow>();
  for (const p of s.detected) {
    rows.set(p.port, {
      port: p.port,
      addr: p.addr,
      local: s.remote ? undefined : `localhost:${p.port}`,
      url: s.remote ? undefined : localUrl(p.port),
      process: p.process,
      pid: p.pid,
      project: p.project,
      termId: p.termId,
      detected: true,
      forwarded: false,
    });
  }
  if (s.remote) {
    for (const f of s.forwards) {
      const base = rows.get(f.remotePort);
      rows.set(f.remotePort, {
        port: f.remotePort,
        addr: base?.addr,
        local: `${f.localAddr}:${f.localPort}`,
        url: f.error ? undefined : forwardUrl(f),
        process: base?.process ?? f.process,
        pid: base?.pid ?? f.pid,
        project: base?.project ?? f.project,
        termId: base?.termId ?? f.termId,
        source: f.source,
        detected: !!base,
        forwarded: !f.error,
        error: f.error,
      });
    }
  }
  return [...rows.values()].sort((a, b) => a.port - b.port);
}

export function portCount(s: PortsState): number {
  return s.remote ? s.forwards.filter((f) => !f.error).length : s.detected.length;
}

export function portChip(s: PortsState): { n: number; forwarded: boolean } {
  const n = portCount(s);
  if (s.remote && n) return { n, forwarded: true };
  return { n: s.detected.length, forwarded: false };
}

export function parseIgnore(text: string): { ports: number[]; invalid: string[] } {
  const out = new Set<number>();
  const invalid: string[] = [];
  const valid = (n: number) => Number.isInteger(n) && n >= 1 && n <= 65535;
  for (const tok of text.split(/[\s,;]+/).filter(Boolean)) {
    const m = tok.match(/^(\d+)(?:-(\d+))?$/);
    const a = m ? Number(m[1]) : NaN;
    const b = m?.[2] ? Number(m[2]) : a;
    if (!valid(a) || !valid(b) || b < a || b - a >= MAX_RANGE) {
      invalid.push(tok);
      continue;
    }
    for (let n = a; n <= b; n++) out.add(n);
  }
  return { ports: [...out].sort((x, y) => x - y), invalid };
}

export function portSettings(s: { portsAutoForward: boolean; portsIgnore: string; portsLocalhostOnly: boolean }): PortSettings {
  return { autoForward: s.portsAutoForward, ignore: parseIgnore(s.portsIgnore).ports, localhostOnly: s.portsLocalhostOnly };
}

export function forwardNotices(s: PortsState): Forward[] {
  return (s.added ?? []).filter((f) => !f.error);
}

export function closedForwards(prev: PortsState, next: PortsState): Forward[] {
  if (!prev.remote || !next.remote) return [];
  const open = new Set(next.forwards.map((f) => f.remotePort));
  return prev.forwards.filter((f) => !f.error && !open.has(f.remotePort));
}

export function parsePort(text: string): number | null | undefined {
  const s = text.trim();
  if (!s) return null;
  const n = Number(s);
  return /^\d+$/.test(s) && n >= 1 && n <= 65535 ? n : undefined;
}
