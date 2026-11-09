import type { UsageLimits, UsageResponse, UsageTotals } from './types';

const tot = (total: number, cost: number | null, messages: number): UsageTotals => ({
  input: Math.round(total * 0.01),
  output: Math.round(total * 0.02),
  cacheRead: Math.round(total * 0.9),
  cacheWrite: Math.round(total * 0.07),
  total,
  cost,
  costPartial: false,
  messages,
  activeMs: messages * 45_000,
});

export function mockUsage(qs: string, now = Date.now()): UsageResponse {
  const q = new URLSearchParams(qs);
  const range = (q.get('range') ?? 'today') as UsageResponse['range'];
  const by = (q.get('by') ?? 'agent') as UsageResponse['by'];
  const n = range === '30d' ? 30 : range === '7d' ? 7 : 1;
  const days = Array.from({ length: n }, (_, i) => {
    const d = new Date(now - (n - 1 - i) * 86_400_000).toISOString().slice(0, 10);
    const v = 20_000_000 + ((i * 7919) % 13) * 9_000_000;
    return { day: d, usage: tot(v, v / 1e6 * 0.3, 200 + i * 3) };
  });
  const sum = days.reduce((a, d) => a + d.usage.total, 0);
  const groups = q.get('line')
    ? [{ key: 'claude:s1', agent: 'claude', sessionId: 's1', lineId: q.get('line'), title: null, project: 'overseer', usage: tot(12_400_000, 3.42, 61) }]
    : [
        { key: 'overseer', project: 'overseer', model: 'claude-opus-5-5', agent: 'claude', usage: tot(sum * 0.6, sum * 0.6 / 1e6 * 0.3, 140) },
        { key: 'api', project: 'api', model: 'gpt-5.4', agent: 'codex', usage: tot(sum * 0.3, sum * 0.3 / 1e6 * 0.2, 60) },
        { key: 'site', project: 'site', model: 'gemini-2.5-pro', agent: 'gemini', usage: tot(sum * 0.1, null, 12) },
      ];
  const reset = (h: number) => new Date(now + h * 3_600_000).toISOString();
  return {
    range,
    by,
    tzOffset: Number(q.get('tzOffset') ?? 0),
    from: new Date(now - n * 86_400_000).toISOString(),
    to: new Date(now).toISOString(),
    totals: tot(sum, sum / 1e6 * 0.3, 900),
    days,
    groups,
    agents: [
      { agent: 'claude', today: tot(days[n - 1].usage.total * 0.7, 6.1, 140), limits: claudeLimits(q.get('claudeLimits') === '1', now, reset) },
      {
        agent: 'codex',
        today: tot(days[n - 1].usage.total * 0.3, 1.2, 60),
        limits: codexCredits(q)
          ? {
              source: 'codex',
              observedAt: new Date(now - 120_000).toISOString(),
              plan: 'go',
              windows: [{ key: 'mo', windowMinutes: 43200, usedPercent: 100, resetsAt: reset(500), stale: false }],
              hit: null,
              credits: { hasCredits: true, unlimited: false, balance: 1272.18 },
              reached: null,
              via: 'codex',
            }
          : {
              source: 'codex',
              observedAt: new Date(now - 120_000).toISOString(),
              plan: 'plus',
              windows: [
                { key: '5h', windowMinutes: 300, usedPercent: 82, resetsAt: reset(2.2), stale: false },
                { key: 'wk', windowMinutes: 10080, usedPercent: 38, resetsAt: reset(80), stale: false },
              ],
              hit: null,
            },
      },
    ],
    indexing: false,
  };
}

function codexCredits(q: URLSearchParams): boolean {
  const page = globalThis.location ? new URLSearchParams(globalThis.location.search) : null;
  return (q.get('codexCredits') ?? page?.get('codexCredits')) === '1';
}

function claudeLimits(on: boolean, now: number, reset: (h: number) => string): UsageLimits {
  if (!on) return { source: null, windows: [], hit: null };
  const auth = globalThis.location ? new URLSearchParams(globalThis.location.search).get('claudeAuth') : null;
  if (auth === 'expired') return { source: null, windows: [], hit: null, auth: 'expired' };
  return {
    source: 'claude',
    observedAt: new Date(now - 60_000).toISOString(),
    plan: 'max',
    windows: [
      { key: '5h', windowMinutes: 300, usedPercent: 83, resetsAt: reset(1.4), stale: false },
      { key: 'wk', windowMinutes: 10080, usedPercent: 47, resetsAt: reset(125), stale: false },
    ],
    hit: null,
    auth: 'ok',
  };
}
