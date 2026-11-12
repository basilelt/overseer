import { splitMinutes, wholeMinutes } from '../duration';
import type { UsageAgent, UsageBy, UsageLimits, UsageRange, UsageWindow } from './types';

import { level, worst, type Level } from '../severity';

export { level, worst, type Level };

export function tzOffset(d: Date = new Date()): number {
  return -d.getTimezoneOffset();
}

export function formatTokens(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return '—';
  const a = Math.abs(n);
  const f = (v: number, unit: string) => `${v >= 100 ? Math.round(v) : Number(v.toFixed(1))}${unit}`;
  if (a < 1000) return String(Math.round(n));
  if (a < 1e6) return f(n / 1e3, 'k');
  if (a < 1e9) return f(n / 1e6, 'M');
  return f(n / 1e9, 'B');
}

export function formatCost(cost: number | null | undefined, partial = false): string {
  if (cost == null || !Number.isFinite(cost)) return '—';
  const plus = partial ? '+' : '';
  if (cost > 0 && cost < 0.01) return `≈<$0.01${plus}`;
  if (cost >= 100) return `≈$${Math.round(cost).toLocaleString('en-US')}${plus}`;
  return `≈$${cost.toFixed(2)}${plus}`;
}

export function formatPct(p: number): string {
  return `${Math.round(p)}%`;
}

export function formatDuration(ms: number): string {
  const total = wholeMinutes(Math.max(0, ms));
  if (total < 1) return '<1m';
  const { d, h, m } = splitMinutes(total);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
  return `${m}m`;
}

export function countdown(resetsAt: string | null | undefined, now: number): string | null {
  if (!resetsAt) return null;
  const t = Date.parse(resetsAt);
  if (!Number.isFinite(t)) return null;
  return t <= now ? '0m' : formatDuration(t - now);
}

export function liveWindows(a: UsageAgent): UsageWindow[] {
  return (a.limits?.windows ?? []).filter((w) => !w.stale);
}

export function authNote(l: UsageLimits | undefined): 'authExpired' | 'authUnavailable' | null {
  if (l?.auth === 'expired') return 'authExpired';
  if (l?.auth === 'unavailable' && !l.windows.some((w) => !w.stale)) return 'authUnavailable';
  return null;
}

export function onCredits(a: UsageAgent): boolean {
  const l = a.limits;
  if (!l || l.hit || l.reached || !(l.credits?.hasCredits || l.credits?.unlimited)) return false;
  return liveWindows(a).some((w) => w.usedPercent >= 100);
}

export function windowLevel(a: UsageAgent, w: UsageWindow): Level {
  const l = level(w.usedPercent);
  return l === 'crit' && onCredits(a) ? 'warn' : l;
}

export function agentLevel(a: UsageAgent): Level {
  if (a.limits?.hit) return 'crit';
  return worst(liveWindows(a).map((w) => windowLevel(a, w)));
}

export interface ChipSegment {
  agent: string;
  name: string;
  parts: string[];
  level: Level;
}

export function chipSegments(agents: UsageAgent[], name: (id: string) => string, limitLabel: string, creditsLabel = 'credits'): ChipSegment[] {
  return agents
    .map((a): ChipSegment | null => {
      const windows = liveWindows(a);
      const nm = name(a.agent);
      if (a.limits?.hit) return { agent: a.agent, name: nm, parts: [limitLabel], level: 'crit' };
      if (windows.length) {
        const parts = windows.map((w) => `${w.key} ${formatPct(w.usedPercent)}`);
        return { agent: a.agent, name: nm, parts: onCredits(a) ? [...parts, creditsLabel] : parts, level: agentLevel(a) };
      }
      if (a.today.total > 0) return { agent: a.agent, name: nm, parts: [formatTokens(a.today.total)], level: 'ok' };
      return null;
    })
    .filter((s): s is ChipSegment => !!s)
    .sort((x, y) => rank(y.level) - rank(x.level) || x.name.localeCompare(y.name));
}

function rank(l: Level): number {
  return l === 'crit' ? 2 : l === 'warn' ? 1 : 0;
}

export function chipText(segs: ChipSegment[]): string {
  return segs.map((s) => `${s.name} ${s.parts.join(' · ')}`).join('  ');
}

export function barHeights(values: number[]): number[] {
  const max = Math.max(0, ...values);
  return values.map((v) => (max > 0 ? Math.max(v > 0 ? 2 : 0, (v / max) * 100) : 0));
}

export function shortDay(day: string, locale: string): string {
  const d = new Date(`${day}T12:00:00Z`);
  return Number.isFinite(d.getTime()) ? d.toLocaleDateString(locale, { weekday: 'short', day: 'numeric', timeZone: 'UTC' }) : day;
}
