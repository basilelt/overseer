import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(arg: string | null) {
  vi.resetModules();
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
  const { render } = await import('svelte/server');
  const { app } = await import('../../state.svelte');
  app.overlayArg = arg;
  const { default: Dialog } = await import('./SettingsDialog.svelte');
  return render(Dialog, { props: { onclose: () => {} } }).body;
}

afterEach(() => vi.unstubAllGlobals());

describe('settings usage tab', () => {
  it('lists Usage as its own category', async () => {
    const html = await rendered(null);
    expect(html.match(/class="cat[^"]*"[^>]*><span>Usage<\/span>/g)).toHaveLength(1);
  });

  it('shows Claude, Codex and Gemini CLI usage in the Usage tab', async () => {
    const html = await rendered('usage');
    expect(html).toContain('Fetch Claude plan limits');
    expect(html).toContain('>Codex</h3>');
    expect(html).toContain('Plan limits from Codex sessions');
    expect(html).toContain('>Gemini CLI</h3>');
    expect(html).toContain('Tokens and cost from Gemini CLI sessions');
  });

  it('keeps usage out of the Agents tab', async () => {
    const html = await rendered('skills');
    expect(html).not.toContain('Fetch Claude plan limits');
    expect(html).not.toContain('Plan limits from Codex sessions');
  });
});
