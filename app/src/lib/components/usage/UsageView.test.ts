import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

async function rendered() {
  vi.resetModules();
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
  const { render } = await import('svelte/server');
  const { app } = await import('../../state.svelte');
  const { MOCK_AGENTS } = await import('../../daemon/mock');
  app.agents = structuredClone(MOCK_AGENTS);
  const { default: View } = await import('./UsageView.svelte');
  return render(View, { props: {} }).body;
}

beforeAll(async () => {
  await Promise.all([import('svelte/server'), import('./UsageView.svelte')]);
}, 60_000);

afterEach(() => vi.unstubAllGlobals());

describe('usage view agents dropdown', () => {
  it('has an Agents dropdown control next to the range tabs, targeted by the tour', async () => {
    const html = await rendered();
    expect(html).toContain('data-tour="usage-agents"');
    expect(html).toContain('title="Show or hide agents in Usage"');
    expect(html).toMatch(/>Agents<!--/);
  });
});
