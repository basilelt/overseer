import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { UsageAgent, UsageWindow } from '../../usage/types';

const NOW = Date.parse('2026-11-20T12:00:00Z');
const zero = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0, cost: 0, costPartial: false, messages: 0, activeMs: 0 };
const win = (key: string, minutes: number, used: number, resetInMin: number, stale = false): UsageWindow => ({
  key,
  windowMinutes: minutes,
  usedPercent: used,
  resetsAt: new Date(NOW + resetInMin * 60_000).toISOString(),
  stale,
});
const codex: UsageAgent = { agent: 'codex', today: zero, limits: { source: 'codex', windows: [win('5h', 300, 40, 240), win('wk', 10080, 5, 8640), win('mo', 43200, 9, 100, true)], hit: null } };
const gemini: UsageAgent = { agent: 'gemini', today: zero, limits: { source: null, windows: [], hit: null } };

async function rendered(agent: string, hidden: string[] = []) {
  vi.resetModules();
  vi.useFakeTimers({ now: NOW });
  const { chipFeed } = await import('../../usage/store.svelte');
  chipFeed.data = { agents: [codex, gemini] } as never;
  const { settings } = await import('../../settings/settings.svelte');
  settings.usageAgentsHidden = hidden;
  const { render } = await import('svelte/server');
  const { default: PlanLimits } = await import('./PlanLimits.svelte');
  return render(PlanLimits, { props: { agent } }).body.replace(/<!--[\s\S]*?-->/g, '');
}

beforeAll(async () => {
  await Promise.all([import('svelte/server'), import('./PlanLimits.svelte')]);
}, 60_000);

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('side panel plan limits', () => {
  it('shows a row per live window of the session agent, named, with its percentage and pace tick', async () => {
    const html = await rendered('codex');
    expect(html).toContain('data-tour="plan-limits"');
    expect(html).toContain('>5-hour</span>');
    expect(html).toContain('>Weekly</span>');
    expect(html).not.toContain('Monthly');
    expect(html).toContain('>40%</span>');
    expect(html).toContain('>5%</span>');
    expect(html.match(/class="pace\b/g)?.length).toBe(2);
    expect(html).toMatch(/class="pace[^"]*" style="left: 19\.99\d*%;"/);
  });

  it('colours a window spent faster than its even pace amber and leaves one behind it grey', async () => {
    const html = await rendered('codex');
    expect(html).toMatch(/class="pct mono warn[^"]*">40%/);
    expect(html).toMatch(/class="pct mono ok[^"]*">5%/);
  });

  it('puts the percentage, even pace, reset countdown and the click hint in the tooltip', async () => {
    const html = await rendered('codex');
    expect(html).toContain('title="5-hour 40% · even pace 20% by now · resets in 4h 00m · Open the usage panel"');
  });

  it('shows nothing for an agent with no plan limits', async () => {
    expect(await rendered('gemini')).not.toContain('class="limits"');
  });

  it('shows nothing for an agent hidden in Usage', async () => {
    expect(await rendered('codex', ['codex'])).not.toContain('class="limits"');
  });
});
