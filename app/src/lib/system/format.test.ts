import { describe, expect, it } from 'vitest';
import { loadLocale, setLocale } from '../i18n.svelte';
import { agentLabel, basename, chartPaths, chipLevel, chipText, formatBytes, formatPct, formatRate, formatUptime, level, mergeHistory, niceMax } from './format';
import { mockSystem } from './mock';
import { systemSession } from './session';
import { st } from './strings';
import type { SysPoint, SysSample } from './types';

const sample = (cpu: number | null, used: number, total = 100): SysSample => ({
  t: 0,
  platform: 'linux',
  supported: true,
  cpu: { total: cpu, cores: [] },
  mem: { total, used, available: total - used, cached: 0, swapTotal: 0, swapUsed: 0 },
});

const pt = (t: number, cpu: number | null = 1): SysPoint => ({ t, cpu, memUsed: 0, memTotal: 1, swapUsed: 0, rx: 0, tx: 0, read: 0, write: 0 });

describe('thresholds', () => {
  it('turns amber at 80 and red at 95', () => {
    expect(level(null)).toBe('ok');
    expect(level(79.9)).toBe('ok');
    expect(level(80)).toBe('warn');
    expect(level(94.9)).toBe('warn');
    expect(level(95)).toBe('crit');
    expect(level(Number.NaN)).toBe('ok');
  });

  it('chip shows CPU and RAM and takes the worst level', () => {
    expect(chipText(sample(23.4, 61))).toBe('CPU 23% · RAM 61%');
    expect(chipLevel(sample(23, 61))).toBe('ok');
    expect(chipLevel(sample(85, 10))).toBe('warn');
    expect(chipLevel(sample(10, 96))).toBe('crit');
    expect(chipLevel(sample(90, 96))).toBe('crit');
    expect(chipText(sample(null, 50))).toBe('CPU — · RAM 50%');
    expect(chipText({ t: 0, platform: 'windows', supported: false })).toBeNull();
    expect(chipText(null)).toBeNull();
  });
});

describe('formatting', () => {
  it('formats bytes, rates, percents and uptime', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1023)).toBe('1023 B');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(450 * 1024 ** 2)).toBe('450 MB');
    expect(formatBytes(16 * 1024 ** 3)).toBe('16 GB');
    expect(formatBytes(-1)).toBe('—');
    expect(formatRate(2048)).toBe('2.0 KB/s');
    expect(formatRate(undefined)).toBe('—');
    expect(formatPct(61.5)).toBe('62%');
    expect(formatUptime(59)).toBe('0m');
    expect(formatUptime(3 * 3600 + 120)).toBe('3h 2m');
    expect(formatUptime(2 * 86400 + 5 * 3600)).toBe('2d 5h');
  });

  it('labels agents by name and project folder', () => {
    expect(agentLabel({ agent: 'claude', cwd: '/home/me/src/overseer/', title: '' })).toBe('Claude in overseer');
    expect(agentLabel({ agent: 'aider', cwd: 'C:\\src\\web', title: '' }, 'dans')).toBe('Aider dans web');
    expect(agentLabel({ agent: null, cwd: '', title: 'zsh' })).toBe('zsh');
    expect(basename('/')).toBe('/');
  });

  it('localizes its own strings', async () => {
    setLocale('fr');
    await loadLocale('fr');
    expect(st('procs', { n: 3 })).toBe('3 processus');
    setLocale('en');
    expect(st('procs', { n: 3 })).toBe('3 processes');
  });
});

describe('charts', () => {
  it('rounds the scale up to 1, 2 or 5 × 10^k', () => {
    expect(niceMax(0)).toBe(1);
    expect(niceMax(0.7)).toBe(1);
    expect(niceMax(13)).toBe(20);
    expect(niceMax(450)).toBe(500);
    expect(niceMax(5000)).toBe(5000);
    expect(niceMax(6001)).toBe(10000);
  });

  it('maps time to x and value to y, clamped to the box', () => {
    const p = chartPaths({ times: [0, 50, 100], values: [0, 50, 200] }, { width: 100, height: 10, max: 100, t1: 100, span: 100 });
    expect(p.line).toBe('M0,10L50,5L100,0');
    expect(p.area).toBe('M0,10L0,10L50,5L100,0L100,10Z');
  });

  it('breaks the line on nulls, gaps and drops points outside the window', () => {
    const p = chartPaths({ times: [-10, 0, 10, 20, 90, 100], values: [5, 1, null, 1, 1, 1] }, { width: 100, height: 10, max: 1, t1: 100, span: 100, gapMs: 30 });
    expect(p.line).toBe('M0,0M20,0M90,0L100,0');
    expect(chartPaths({ times: [], values: [] }, { width: 1, height: 1, max: 0, t1: 0 })).toEqual({ line: '', area: '' });
  });
});

describe('history', () => {
  it('appends new points, replaces overlap and trims the window', () => {
    const a = [pt(1000), pt(3000)];
    expect(mergeHistory(a, [pt(3000), pt(5000)], 5000).map((p) => p.t)).toEqual([1000, 3000, 5000]);
    expect(mergeHistory(a, [pt(2000, 9), pt(4000)], 4000).map((p) => [p.t, p.cpu])).toEqual([
      [1000, 1],
      [2000, 9],
      [4000, 1],
    ]);
    expect(mergeHistory(a, [], 700_000, 600_000).map((p) => p.t)).toEqual([]);
    expect(mergeHistory(a, [], 3000)).toBe(a);
  });

  it('mock daemon returns a full window, then only newer points', () => {
    const full = mockSystem('', 1_000_000);
    expect(full.history).toHaveLength(300);
    const last = full.history[full.history.length - 1].t;
    expect(mockSystem(`since=${last}`, 1_004_000).history.map((p) => p.t)).toEqual([last + 2000, last + 4000]);
  });

  it('session key only while connected', () => {
    const conn = { phase: 'connected' as const, target: { kind: 'ssh' as const, host: 'box', label: 'box' } };
    expect(systemSession(true, conn, true)).toBe('ssh:box');
    expect(systemSession(false, conn, true)).toBeNull();
    expect(systemSession(true, { ...conn, phase: 'reconnecting' }, true)).toBeNull();
  });
});
