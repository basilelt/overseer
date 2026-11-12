import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(phone: boolean) {
  vi.resetModules();
  vi.doMock('../../mobile/phone.svelte', () => ({ phoneOrMobile: () => phone }));
  const { render } = await import('svelte/server');
  const { default: Host } = await import('./PanelShellHost.test.svelte');
  return render(Host, { props: { onclose: () => {} } }).body;
}

afterEach(() => {
  vi.doUnmock('../../mobile/phone.svelte');
  vi.restoreAllMocks();
});

describe('PanelShell', () => {
  it('renders a full-screen sheet with a close button and compact content on a phone', async () => {
    const h = await rendered(true);
    expect(h).toContain('class="sheet ');
    expect(h).toContain('aria-label="Panel title"');
    expect(h).toContain('aria-label="Close"');
    expect(h).toContain('content:compact');
    expect(h).not.toContain('Open in tab');
  });

  it('renders a modal with an open-in-tab footer on desktop', async () => {
    const h = await rendered(false);
    expect(h).not.toContain('class="sheet ');
    expect(h).toContain('content:full');
    expect(h).toContain('Open in tab');
  });
});
