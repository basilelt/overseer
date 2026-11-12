import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { AgentInfo } from '../../types';

const agent = (id: string, name: string, installed = true): AgentInfo => ({ id, name, installed, efforts: [], resume: false, verified: true, sessions: true, turns: true });

async function rendered(agents: AgentInfo[]) {
  vi.resetModules();
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {} });
  const { render } = await import('svelte/server');
  const { app } = await import('../../state.svelte');
  app.agents = agents;
  app.connected = true;
  app.overlayArg = 'skills';
  const { default: Dialog } = await import('./SettingsDialog.svelte');
  return render(Dialog, { props: { onclose: () => {} } }).body;
}

const options = (html: string, label: string) => [...(html.match(new RegExp(`<select[^>]*aria-label="${label}"[^>]*>(.*?)</select>`))?.[1] ?? '').matchAll(/>([^<]+)<\/option>/g)].map((m) => m[1]);

beforeAll(async () => {
  await Promise.all([import('svelte/server'), import('./SettingsDialog.svelte')]);
}, 60_000);

afterEach(() => vi.unstubAllGlobals());

describe('settings skills and plugins', () => {
  it('offers each tool for the installed agents that support it', async () => {
    const html = await rendered([agent('claude', 'Claude Code'), agent('amp', 'Amp'), agent('codex', 'Codex', false), agent('shell', 'Shell')]);
    expect(html).toContain('Skills and plugins');
    expect(options(html, 'RTK Agent')).toEqual(['Claude Code']);
    expect(options(html, 'Caveman Agent')).toEqual(['Claude Code', 'Amp']);
    expect(options(html, 'Ponytail Agent')).toEqual(['Claude Code', 'Amp']);
    expect(html.match(/>Install<\/button>/g)).toHaveLength(3);
  });

  it('says so when no installed agent supports a tool', async () => {
    const html = await rendered([agent('amp', 'Amp')]);
    expect(options(html, 'RTK Agent')).toEqual([]);
    expect(html).toContain('No supported agent');
  });
});
