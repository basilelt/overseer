import { splitMinutes } from '../duration';
import type { SysAgent, SysPoint, SysSample } from './types';

import { level, worst, type Level } from '../severity';

import { basename } from '../paths';

import { SHORT_NAMES } from '../agents';

export { level, type Level };
export const WINDOW_MS = 600_000;

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

export function formatBytes(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n) || n < 0) return '—';
  let i = 0;
  let v = n;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const s = i === 0 || v >= 10 ? String(Math.round(v)) : v.toFixed(1);
  return `${s} ${UNITS[i]}`;
}

export function formatRate(n: number | null | undefined): string {
  const b = formatBytes(n);
  return b === '—' ? b : `${b}/s`;
}

export function formatPct(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return '—';
  return `${Math.round(n)}%`;
}

export function formatUptime(sec: number | null | undefined): string {
  if (sec == null || !Number.isFinite(sec) || sec < 0) return '—';
  const { d, h, m } = splitMinutes(Math.floor(sec / 60));
  if (d) return `${d}d ${h}h`;
  if (h) return `${h}h ${m}m`;
  return `${m}m`;
}

export function memPct(s: SysSample | null | undefined): number | null {
  const m = s?.mem;
  return m && m.total > 0 ? (m.used / m.total) * 100 : null;
}

export function cpuPct(s: SysSample | null | undefined): number | null {
  return s?.cpu?.total ?? null;
}

export function chipText(s: SysSample | null | undefined): string | null {
  if (!s?.supported) return null;
  return `CPU ${formatPct(cpuPct(s))} · RAM ${formatPct(memPct(s))}`;
}

export function chipLevel(s: SysSample | null | undefined): Level {
  return worst([level(cpuPct(s)), level(memPct(s))]);
}

export function niceMax(max: number): number {
  if (!Number.isFinite(max) || max <= 0) return 1;
  const p = 10 ** Math.floor(Math.log10(max));
  const f = max / p;
  const step = f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10;
  return step * p;
}

export interface Series {
  values: (number | null)[];
  times: number[];
}

export interface ChartBox {
  width: number;
  height: number;
  max: number;
  t1: number;
  span?: number;
  gapMs?: number;
}

export function chartPaths(s: Series, box: ChartBox): { line: string; area: string } {
  const span = box.span ?? WINDOW_MS;
  const t0 = box.t1 - span;
  const gap = box.gapMs ?? 6000;
  const max = box.max > 0 ? box.max : 1;
  const x = (t: number) => ((t - t0) / span) * box.width;
  const y = (v: number) => box.height - (Math.min(Math.max(v, 0), max) / max) * box.height;
  const r = (n: number) => Math.round(n * 10) / 10;
  let line = '';
  let area = '';
  let run: [number, number][] = [];
  const flush = () => {
    if (run.length) {
      line += run.map(([a, b], i) => `${i ? 'L' : 'M'}${r(a)},${r(b)}`).join('');
      area += `M${r(run[0][0])},${box.height}` + run.map(([a, b]) => `L${r(a)},${r(b)}`).join('') + `L${r(run[run.length - 1][0])},${box.height}Z`;
    }
    run = [];
  };
  let prevT: number | null = null;
  for (let i = 0; i < s.values.length; i++) {
    const v = s.values[i];
    const t = s.times[i];
    if (t < t0) continue;
    if (v == null || !Number.isFinite(v) || (prevT != null && t - prevT > gap)) flush();
    if (v != null && Number.isFinite(v)) run.push([x(t), y(v)]);
    prevT = t;
  }
  flush();
  return { line, area };
}

export function mergeHistory(prev: SysPoint[], add: SysPoint[], now: number, windowMs = WINDOW_MS): SysPoint[] {
  if (!add.length) return prev.length && prev[0].t < now - windowMs ? prev.filter((p) => p.t >= now - windowMs) : prev;
  const last = prev.length ? prev[prev.length - 1].t : -Infinity;
  const fresh = add.filter((p) => p.t > last);
  const out = add[0].t <= last ? [...prev.filter((p) => p.t < add[0].t), ...add] : [...prev, ...fresh];
  return out.filter((p) => p.t >= now - windowMs);
}


export function agentLabel(a: Pick<SysAgent, 'agent' | 'cwd' | 'title'>, inWord = 'in'): string {
  const name = a.agent ? (SHORT_NAMES[a.agent] ?? a.agent.charAt(0).toUpperCase() + a.agent.slice(1)) : a.title || SHORT_NAMES.shell;
  return a.cwd ? `${name} ${inWord} ${basename(a.cwd)}` : name;
}
