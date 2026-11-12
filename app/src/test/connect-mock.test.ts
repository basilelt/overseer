import { afterEach, describe, expect, it } from 'vitest';
import { app } from '../lib/state.svelte';
import { connectMock } from './connect-mock';
import { quietMock } from './quiet-mock';

afterEach(() => app.disconnect());

describe('quietMock', () => {
  it('builds a daemon without live events or latency, honouring extra options', () => {
    expect(quietMock().lines.length).toBeGreaterThan(0);
    expect(quietMock({ scale: 3 }).lines.length).toBeGreaterThan(quietMock().lines.length);
  });
});

describe('connectMock', () => {
  it('connects the app to the given daemon and returns it', async () => {
    const m = quietMock();
    expect(await connectMock(m)).toBe(m);
    expect(app.connected).toBe(true);
    expect(app.data.lines.length).toBe(m.lines.length);
  });
});
