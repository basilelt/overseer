import type { SysPoint, SysResponse, SysSample } from './types';

const GB = 1024 ** 3;
const STEP = 2000;

function wave(t: number, period: number, lo: number, hi: number): number {
  return lo + ((Math.sin(t / period) + 1) / 2) * (hi - lo);
}

function point(t: number): SysPoint {
  return {
    t,
    cpu: Math.round(wave(t, 40_000, 8, 72) * 10) / 10,
    memUsed: Math.round(wave(t, 90_000, 9, 12) * GB),
    memTotal: 16 * GB,
    swapUsed: 0.2 * GB,
    rx: Math.round(wave(t, 25_000, 2e4, 2e6)),
    tx: Math.round(wave(t, 33_000, 1e4, 4e5)),
    read: Math.round(wave(t, 50_000, 0, 8e6)),
    write: Math.round(wave(t, 21_000, 1e5, 3e7)),
  };
}

export function mockSystem(qs: string, now = Date.now()): SysResponse {
  const since = Number(new URLSearchParams(qs).get('since') ?? NaN);
  const end = Math.floor(now / STEP) * STEP;
  const history: SysPoint[] = [];
  for (let t = end - 299 * STEP; t <= end; t += STEP) if (!(t <= since)) history.push(point(t));
  const p = point(end);
  const current: SysSample = {
    t: end,
    platform: 'linux',
    supported: true,
    uptimeSec: 352_000,
    load: [1.42, 1.18, 0.97],
    cpu: { total: p.cpu, cores: Array.from({ length: 8 }, (_, i) => Math.round(wave(end + i * 7000, 20_000, 2, 98))) },
    pressure: { cpu: 1.2, memory: 0, io: 0.4 },
    mem: { total: p.memTotal, used: p.memUsed, available: p.memTotal - p.memUsed, cached: 3.1 * GB, swapTotal: 4 * GB, swapUsed: p.swapUsed },
    net: [{ name: 'eth0', rx: p.rx, tx: p.tx }],
    disks: [{ name: 'nvme0n1', read: p.read, write: p.write }],
    fs: [
      { mount: '/', device: '/dev/nvme0n1p2', fstype: 'ext4', total: 512 * GB, used: 431 * GB, avail: 81 * GB },
      { mount: '/boot/efi', device: '/dev/nvme0n1p1', fstype: 'vfat', total: 0.5 * GB, used: 0.05 * GB, avail: 0.45 * GB },
    ],
    top: {
      cpu: [
        { pid: 4242, name: 'claude', cpu: 23.5, rss: 450 * 1024 ** 2 },
        { pid: 5120, name: 'node', cpu: 12.1, rss: 310 * 1024 ** 2 },
        { pid: 811, name: 'overseer', cpu: 0.4, rss: 18 * 1024 ** 2 },
      ],
      mem: [
        { pid: 4242, name: 'claude', cpu: 23.5, rss: 450 * 1024 ** 2 },
        { pid: 5120, name: 'node', cpu: 12.1, rss: 310 * 1024 ** 2 },
      ],
    },
    agents: [{ termId: 't1', lineId: 'l1', agent: 'claude', title: 'claude', cwd: '/home/me/src/overseer', pid: 4242, procs: 3, cpu: 3.2, rss: 450 * 1024 ** 2 }],
  };
  return { intervalMs: STEP, windowMs: 300 * STEP, running: true, current, history };
}
