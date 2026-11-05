import { afterEach, describe, expect, it, vi } from 'vitest';

const store = new Map<string, string>();

async function rendered(demo: boolean, props: Record<string, unknown> = {}) {
  store.clear();
  if (demo) store.set('overseer.demo', '1');
  vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) });
  vi.resetModules();
  const { render } = await import('svelte/server');
  const { default: DemoPill } = await import('./DemoPill.svelte');
  return render(DemoPill, { props }).body.replace(/<!--.*?-->/g, '');
}

afterEach(() => vi.unstubAllGlobals());

describe('DemoPill', () => {
  it('renders nothing outside the demo', async () => {
    expect(await rendered(false)).not.toContain('Demo mode');
  });

  it('shows Demo mode with an Exit demo action in the demo', async () => {
    const html = await rendered(true);
    expect(html).toContain('Demo mode');
    expect(html).toContain('data-tour="demo"');
    expect(html).toMatch(/<button[^>]*aria-label="Exit demo"[^>]*>\s*Exit demo/);
  });

  it('keeps an accessible Exit demo button with an icon on the phone', async () => {
    const html = await rendered(true, { compact: true });
    expect(html).toMatch(/class="demo[^"]*compact/);
    expect(html).toMatch(/<button[^>]*aria-label="Exit demo"[^>]*>\s*<svg/);
  });
});
