import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import SessionCost from './SessionCost.svelte';
import type { SessionCost as Cost } from '../../types';

const base: Cost = {
  input: 12,
  cacheWrite: 61_000,
  cacheRead: 2_400_000,
  output: 30_000,
  cost: 1.5,
  partial: false,
  model: 'claude-sonnet-5-5',
  effort: null,
  pricesAsOf: '2026-11-11',
  models: [
    { model: 'claude-opus-5-5', input: 10, cacheWrite: 1000, cacheRead: 2_000_000, output: 20_000, cost: 1.2 },
    { model: 'claude-sonnet-5-5', input: 2, cacheWrite: 60_000, cacheRead: 400_000, output: 10_000, cost: 0.3 },
  ],
  switches: [{ at: '2026-11-11T10:00:00Z', from: 'claude-opus-5-5', to: 'claude-sonnet-5-5', cacheWrite: 60_000, cost: 0.16 }],
};

function stripComments(html: string): string {
  for (let prev = ''; prev !== html; ) {
    prev = html;
    html = html.replace(/<!--[\s\S]*?-->/g, '');
  }
  return html;
}

const html = (c: Cost) => stripComments(render(SessionCost, { props: { cost: c } }).body);

describe('SessionCost', () => {
  it('lists each model with its cost and its token split', () => {
    const h = html(base);
    expect(h).toContain('claude-opus-5-5');
    expect(h).toContain('≈$1.20');
    expect(h).toContain('in 10 · cache write 1k · cache read 2M · out 20k');
    expect(h).toContain('in 2 · cache write 60k · cache read 400k · out 10k');
  });

  it('shows a model switch as its own cost event', () => {
    expect(html(base)).toMatch(/switched claude-opus-5-5 → claude-sonnet-5-5: ≈\$0\.16 to rebuild the cache/);
  });

  it('says the cost is at list prices of a dated table', () => {
    expect(html(base)).toContain('API list prices (2026-11-11)');
  });

  it('warns when a model has no price, and shows a dash for it', () => {
    const h = html({ ...base, partial: true, models: [{ ...base.models[0], model: 'mystery', cost: null }], switches: [] });
    expect(h).toContain('Some models have no price');
    expect(h).toContain('—');
    expect(h).not.toContain('switched');
  });
});
