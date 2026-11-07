import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(stored: Record<string, unknown>) {
  vi.resetModules();
  vi.stubGlobal('localStorage', { getItem: (k: string) => (k === 'overseer.settings' ? JSON.stringify(stored) : null), setItem: () => {} });
  const { render } = await import('svelte/server');
  const { default: Dialog } = await import('./SettingsDialog.svelte');
  return render(Dialog, { props: { onclose: () => {} } }).body;
}

const select = (html: string, label: string) => html.match(new RegExp(`<select[^>]*aria-label="${label}"[^>]*>(.*?)</select>`))![1];

afterEach(() => vi.unstubAllGlobals());

describe('settings palettes', () => {
  it('offers System, Light and Dark only as the theme', async () => {
    const html = await rendered({});
    const seg = html.match(/<div class="seg themes[^"]*"[^>]*>(.*?)<\/div>/)![1];
    expect([...seg.matchAll(/<button[^>]*>([^<]*)<\/button>/g)].map((m) => m[1])).toEqual(['System', 'Light', 'Dark']);
    expect(seg).toMatch(/aria-checked="true"[^>]*>System/);
  });

  it('lists the default light palette and the three dark palettes', async () => {
    const html = await rendered({});
    expect([...select(html, 'Light palette').matchAll(/>([^<]+)<\/option>/g)].map((m) => m[1])).toEqual(['Default light']);
    expect([...select(html, 'Dark palette').matchAll(/>([^<]+)<\/option>/g)].map((m) => m[1])).toEqual(['Default dark', 'Graphite', 'Solarized Soft']);
  });

  it('migrates a saved Graphite theme to the Dark palette choice', async () => {
    const html = await rendered({ theme: 'graphite' });
    expect(select(html, 'Dark palette')).toMatch(/<option[^>]*value="graphite"[^>]*selected/);
    expect(html).toMatch(/aria-checked="true"[^>]*>Dark/);
  });

  it('lists custom presets under their mode and shows Edit and Delete only for a selected custom one', async () => {
    const preset = { id: 'custom:a', name: 'Paper', mode: 'light', colors: { accent: '#2563eb', bg: '#f5f0e6', surface: '#ffffff', border: '#e4e4e7', text: '#18181b', 'text-2': '#52525b' } };
    const plain = await rendered({ customPresets: [preset] });
    expect(select(plain, 'Light palette')).toContain('Paper');
    expect(select(plain, 'Dark palette')).not.toContain('Paper');
    expect(plain).not.toContain('>Delete<');
    const selected = await rendered({ customPresets: [preset], lightPreset: 'custom:a' });
    expect(selected).toContain('>Edit<');
    expect(selected).toContain('>Delete<');
  });
});
