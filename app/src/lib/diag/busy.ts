export const STALL_MS = 500;
export const LATE_MS = 1000;
const BURST_MS = 100;
const TOP = 5;

interface Stat {
  ms: number;
  n: number;
  max: number;
  bytes: number;
}

export interface Report {
  late: number;
  busy: string;
}

const secs = (ms: number) => `${(ms / 1000).toFixed(1)} s`;

export class BusyMeter {
  private stats = new Map<string, Stat>();
  private answered = -Infinity;

  constructor(private now: () => number = () => performance.now()) {}

  add(name: string, ms: number, bytes = 0): void {
    let s = this.stats.get(name);
    if (!s) this.stats.set(name, (s = { ms: 0, n: 0, max: 0, bytes: 0 }));
    s.ms += ms;
    s.n++;
    s.bytes += bytes;
    if (ms > s.max) s.max = ms;
  }

  run<T>(name: string, fn: () => T, bytes = 0): T {
    const t = this.now();
    try {
      return fn();
    } finally {
      this.add(name, this.now() - t, bytes);
    }
  }

  since(name: string, bytes = 0): () => void {
    const t = this.now();
    return () => this.add(name, this.now() - t, bytes);
  }

  // Pings held up behind a stall arrive together: only the first one tells how long the page was
  // stuck, and it takes the figures with it.
  take(sentAt: number | undefined, wall: number): Report {
    const t = this.now();
    const burst = t - this.answered < BURST_MS;
    this.answered = t;
    const late = burst || sentAt === undefined ? 0 : Math.max(0, Math.round(wall - sentAt));
    const all = [...this.stats].sort((a, b) => b[1].ms - a[1].ms);
    this.stats.clear();
    const stalled = late >= LATE_MS || all.some(([, s]) => s.max >= STALL_MS);
    if (!stalled) return { late: 0, busy: '' };
    const busy = all
      .filter(([, s]) => s.ms >= 1)
      .slice(0, TOP)
      .map(([name, s]) => `${name} ${secs(s.ms)} x${s.n} (max ${secs(s.max)}${s.bytes ? `, ${(s.bytes / 1048576).toFixed(1)} MB` : ''})`)
      .join(', ');
    return { late, busy };
  }
}

export const busy = new BusyMeter();
