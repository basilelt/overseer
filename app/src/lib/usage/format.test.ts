import { describe, expect, it } from 'vitest';
import { PACE_MARGIN, gaugeLevel, pacePercent, agentLevel, authNote, onCredits, windowLevel, barHeights, chipSegments, chipText, countdown, formatCost, formatDuration, formatTokens, tzOffset } from './format';
import { level, worst } from '../severity';
import type { UsageAgent, UsageTotals } from './types';
import { mockUsage } from './mock';
import { creditsNote, windowLabel } from './strings';

const zero: UsageTotals = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0, cost: 0, costPartial: false, messages: 0, activeMs: 0 };
const agent = (id: string, total: number, windows: [string, number, boolean?][] = [], hit = false): UsageAgent => ({
  agent: id,
  today: { ...zero, total },
  limits: {
    source: windows.length ? id : null,
    windows: windows.map(([key, usedPercent, stale]) => ({ key, windowMinutes: 300, usedPercent, resetsAt: null, stale: !!stale })),
    hit: hit ? { at: '2026-10-29T10:00:00Z', message: 'limit' } : null,
  },
});
const name = (id: string) => id[0].toUpperCase() + id.slice(1);

describe('usage thresholds', () => {
  it('turns amber at 80 % and red at 95 %', () => {
    expect(level(0)).toBe('ok');
    expect(level(79.9)).toBe('ok');
    expect(level(80)).toBe('warn');
    expect(level(94.9)).toBe('warn');
    expect(level(95)).toBe('crit');
    expect(level(120)).toBe('crit');
    expect(level(null)).toBe('ok');
    expect(level(Number.NaN)).toBe('ok');
    expect(worst(['ok', 'warn'])).toBe('warn');
    expect(worst(['warn', 'crit', 'ok'])).toBe('crit');
    expect(worst([])).toBe('ok');
  });

  it('rates an agent by its worst live window, ignoring windows that already reset', () => {
    expect(agentLevel(agent('codex', 0, [['5h', 30], ['wk', 85]]))).toBe('warn');
    expect(agentLevel(agent('codex', 0, [['5h', 99, true], ['wk', 10]]))).toBe('ok');
    expect(agentLevel(agent('claude', 0, [], true))).toBe('crit');
  });
});

describe('usage chip', () => {
  it('shows windows when known, tokens today otherwise, worst first', () => {
    const segs = chipSegments([agent('claude', 1_234_567), agent('codex', 50, [['5h', 29], ['wk', 96]]), agent('gemini', 0)], name, 'limit');
    expect(segs.map((s) => [s.agent, s.parts, s.level])).toEqual([
      ['codex', ['5h 29%', 'wk 96%'], 'crit'],
      ['claude', ['1.2M'], 'ok'],
    ]);
    expect(chipText(segs)).toBe('Codex 5h 29% · wk 96%  Claude 1.2M');
  });

  it('flags a reached limit and hides idle agents', () => {
    expect(chipSegments([agent('claude', 10, [], true)], name, 'limit')[0]).toMatchObject({ parts: ['limit'], level: 'crit' });
    expect(chipSegments([agent('claude', 0)], name, 'limit')).toEqual([]);
  });
});

describe('plan quota spent but paid by credits', () => {
  const spent = (credits: UsageAgent['limits']['credits'], extra: Partial<UsageAgent['limits']> = {}): UsageAgent => {
    const a = agent('codex', 50, [['mo', 100]]);
    return { ...a, limits: { ...a.limits, plan: 'go', credits, ...extra } };
  };
  const paid = { hasCredits: true, unlimited: false, balance: 1272.182153 };

  it('rates a spent window as a warning, not a reached limit, while credits remain', () => {
    const a = spent(paid);
    expect(onCredits(a)).toBe(true);
    expect(windowLevel(a, a.limits.windows[0])).toBe('warn');
    expect(agentLevel(a)).toBe('warn');
  });

  it('keeps a spent window critical without credits, when blocked or when a limit was hit', () => {
    for (const a of [
      spent(null),
      spent({ hasCredits: false, unlimited: false, balance: 0 }),
      spent(paid, { reached: 'workspace_member_usage_limit_reached' }),
      spent(paid, { hit: { at: '2026-11-09T08:00:00Z', message: 'limit' } }),
    ]) {
      expect(onCredits(a)).toBe(false);
      expect(agentLevel(a)).toBe('crit');
    }
  });

  it('does not mention credits while plan quota is left', () => {
    const a = agent('codex', 50, [['mo', 40]]);
    expect(onCredits({ ...a, limits: { ...a.limits, credits: paid } })).toBe(false);
  });

  it('adds credits to the chip and states the balance left', () => {
    const a = spent(paid);
    expect(chipSegments([a], name, 'limit', 'credits')[0]).toMatchObject({ parts: ['mo 100%', 'credits'], level: 'warn' });
    expect(creditsNote(a)).toContain('1,272.18');
    expect(creditsNote(spent({ hasCredits: false, unlimited: true, balance: null }))).toBe('Plan quota used up, running on credits');
  });

  it('serves the credits state from the mock as the daemon does', () => {
    const r = mockUsage('range=today&by=agent&codexCredits=1', Date.parse('2026-11-09T09:00:00Z'));
    const codex = r.agents.find((a) => a.agent === 'codex')!;
    expect(codex.limits.credits).toEqual({ hasCredits: true, unlimited: false, balance: 1272.18 });
    expect(codex.limits.reached).toBeNull();
    expect(onCredits(codex)).toBe(true);
  });
});

describe('Claude plan limits', () => {
  it('shows the real 5h and weekly limits in the chip', () => {
    const r = mockUsage('range=today&by=agent&claudeLimits=1', Date.parse('2026-10-29T18:00:00Z'));
    const claude = r.agents.find((a) => a.agent === 'claude')!;
    const segs = chipSegments([claude], name, 'limit');
    expect(chipText(segs)).toBe('Claude 5h 83% · wk 47%');
    expect(segs[0].level).toBe('warn');
    expect(claude.limits.windows.map((w) => countdown(w.resetsAt, Date.parse('2026-10-29T18:00:00Z')))).toEqual(['1h 24m', '5d 5h']);
    expect(authNote(claude.limits)).toBeNull();
  });

  it('falls back to tokens and explains an expired login', () => {
    const expired: UsageAgent = { ...agent('claude', 1_200_000), limits: { source: null, windows: [], hit: null, auth: 'expired' } };
    expect(chipText(chipSegments([expired], name, 'limit'))).toBe('Claude 1.2M');
    expect(authNote(expired.limits)).toBe('authExpired');
    const stale: UsageAgent = { ...agent('claude', 0, [['5h', 40]]), limits: { ...agent('claude', 0, [['5h', 40]]).limits, auth: 'unavailable' } };
    expect(authNote(stale.limits)).toBeNull();
    expect(authNote({ ...stale.limits, windows: [] })).toBe('authUnavailable');
  });
});

describe('usage formatting', () => {
  it('formats token counts compactly', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(999)).toBe('999');
    expect(formatTokens(1000)).toBe('1k');
    expect(formatTokens(12_345)).toBe('12.3k');
    expect(formatTokens(123_456)).toBe('123k');
    expect(formatTokens(4_197_314_180)).toBe('4.2B');
    expect(formatTokens(null)).toBe('—');
  });

  it('marks costs as estimates', () => {
    expect(formatCost(3.456)).toBe('≈$3.46');
    expect(formatCost(0.004)).toBe('≈<$0.01');
    expect(formatCost(1769.01)).toBe('≈$1,769');
    expect(formatCost(2, true)).toBe('≈$2.00+');
    expect(formatCost(null)).toBe('—');
    expect(formatCost(0)).toBe('≈$0.00');
  });

  it('formats durations and reset countdowns', () => {
    expect(formatDuration(30_000)).toBe('<1m');
    expect(formatDuration(12 * 60_000)).toBe('12m');
    expect(formatDuration((2 * 60 + 5) * 60_000)).toBe('2h 05m');
    expect(formatDuration((50 * 60 + 5) * 60_000)).toBe('2d 2h');
    const now = Date.parse('2026-10-29T18:00:00Z');
    expect(countdown('2026-10-29T20:14:00Z', now)).toBe('2h 14m');
    expect(countdown('2026-10-29T17:00:00Z', now)).toBe('0m');
    expect(countdown(null, now)).toBeNull();
    expect(countdown('nope', now)).toBeNull();
  });

  it('sends the timezone as minutes east of UTC', () => {
    expect(tzOffset({ getTimezoneOffset: () => -120 } as Date)).toBe(120);
    expect(tzOffset({ getTimezoneOffset: () => 300 } as Date)).toBe(-300);
  });

  it('scales bars to the busiest day and keeps tiny days visible', () => {
    expect(barHeights([0, 50, 100])).toEqual([0, 50, 100]);
    expect(barHeights([1, 1000])).toEqual([2, 100]);
    expect(barHeights([0, 0])).toEqual([0, 0]);
  });
});

describe('plan limit pace', () => {
  const now = Date.parse('2026-11-20T12:00:00Z');
  const win = (usedPercent: number, minutes: number, resetInMin: number | null, stale = false) => ({
    key: '5h',
    windowMinutes: minutes,
    usedPercent,
    resetsAt: resetInMin == null ? null : new Date(now + resetInMin * 60_000).toISOString(),
    stale,
  });
  const of = (w: ReturnType<typeof win>): UsageAgent => ({ agent: 'codex', today: zero, limits: { source: 'codex', windows: [w], hit: null } });

  it('places the even pace at the share of the window already elapsed', () => {
    expect(pacePercent(win(0, 300, 240), now)).toBeCloseTo(20);
    expect(pacePercent(win(0, 10080, 10080), now)).toBe(0);
    expect(pacePercent(win(0, 43200, 21600), now)).toBeCloseTo(50);
  });

  it('clamps the pace when the reset is further away than the window', () => {
    expect(pacePercent(win(0, 300, 400), now)).toBe(0);
  });

  it('has no pace without a future reset, for a stale window or a window of no length', () => {
    expect(pacePercent(win(10, 300, null), now)).toBeNull();
    expect(pacePercent(win(10, 300, 0), now)).toBeNull();
    expect(pacePercent(win(10, 300, -5), now)).toBeNull();
    expect(pacePercent(win(10, 300, 60, true), now)).toBeNull();
    expect(pacePercent(win(10, 0, 60), now)).toBeNull();
    expect(pacePercent({ ...win(10, 300, 60), resetsAt: 'soon' }, now)).toBeNull();
  });

  it('turns a gauge amber when usage runs more than the margin ahead of the even pace', () => {
    expect(gaugeLevel(of(win(40, 300, 240)), win(40, 300, 240), now)).toBe('warn');
    expect(gaugeLevel(of(win(19 + PACE_MARGIN, 300, 240)), win(19 + PACE_MARGIN, 300, 240), now)).toBe('ok');
    expect(gaugeLevel(of(win(10, 300, 240)), win(10, 300, 240), now)).toBe('ok');
    expect(gaugeLevel(of(win(40, 300, null)), win(40, 300, null), now)).toBe('ok');
  });

  it('keeps the threshold level when it is already amber or red', () => {
    expect(gaugeLevel(of(win(97, 300, 10)), win(97, 300, 10), now)).toBe('crit');
    expect(gaugeLevel(of(win(85, 300, 10)), win(85, 300, 10), now)).toBe('warn');
  });

  it('names the known windows and keeps other keys as they are', () => {
    expect(windowLabel('5h')).toBe('5-hour');
    expect(windowLabel('wk')).toBe('Weekly');
    expect(windowLabel('mo')).toBe('Monthly');
    expect(windowLabel('3d')).toBe('3d');
  });
});
