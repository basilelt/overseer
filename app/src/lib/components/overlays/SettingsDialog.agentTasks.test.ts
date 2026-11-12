import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(stored: Record<string, unknown> | null) {
  vi.resetModules();
  vi.stubGlobal('localStorage', { getItem: () => (stored ? JSON.stringify(stored) : null), setItem: () => {} });
  const { render } = await import('svelte/server');
  const { app } = await import('../../state.svelte');
  app.overlayArg = 'skills';
  const { default: Dialog } = await import('./SettingsDialog.svelte');
  return render(Dialog, { props: { onclose: () => {} } }).body;
}

const input = (html: string) => html.match(/<input[^>]*data-tour="agent-tasks"[^>]*>/)?.[0];

afterEach(() => vi.unstubAllGlobals());

describe('settings agent tasks switch', () => {
  it('shows the switch in Agents, Defaults, on by default', async () => {
    const html = await rendered(null);
    expect(html).toContain('Agents file what they find as tasks');
    expect(html).toContain('fixes what it finds itself');
    expect(input(html)).toMatch(/checked/);
  });

  it('shows the switch off when agents fix what they find', async () => {
    const html = await rendered({ agentTasks: false });
    expect(input(html)).toContain('type="checkbox"');
    expect(input(html)).not.toMatch(/checked/);
  });
});
