import { afterEach, describe, expect, it, vi } from 'vitest';
import { MOCK_AGENTS } from '../../mock';
import type { AgentInfo } from '../../types';

async function rendered(connected: boolean, agents: AgentInfo[] = []) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  vi.spyOn(app, 'connected', 'get').mockReturnValue(connected);
  app.agents = agents;
  const { render } = await import('svelte/server');
  const { default: Palette } = await import('./CommandPalette.svelte');
  return render(Palette, { props: { onclose: () => {} } }).body;
}

function section(html: string, name: string): string {
  const at = html.indexOf(`>${name}</li>`);
  expect(at).toBeGreaterThan(-1);
  const next = html.indexOf('class="section"', at);
  return html.slice(at, next < 0 ? undefined : next);
}

afterEach(() => vi.restoreAllMocks());

describe('command palette views', () => {
  it('groups every view under its own Views section', async () => {
    const views = section(await rendered(true), 'Views');
    for (const label of ['Review changes', 'Open browser', 'Open the Git graph', 'Open the worktrees', 'Open PR reviews']) expect(views).toContain(label);
  });

  it('keeps the views out of the Actions section', async () => {
    const actions = section(await rendered(true), 'Actions');
    expect(actions).not.toContain('Open the Git graph');
    expect(actions).not.toContain('Open PR reviews');
  });

  it('still lists the local views when disconnected', async () => {
    const views = section(await rendered(false), 'Views');
    expect(views).toContain('Open the Git graph');
    expect(views).not.toContain('Review changes');
  });

  it('lists Install skills and plugins under Actions only when connected', async () => {
    expect(section(await rendered(true), 'Actions')).toContain('Install skills and plugins');
    expect(await rendered(false)).not.toContain('Install skills and plugins');
  });

  it('offers to install each absent agent that has an install command', async () => {
    const actions = section(await rendered(true, MOCK_AGENTS), 'Actions');
    for (const name of ['opencode', 'Amp', 'Cursor Agent']) expect(actions).toContain(`Install ${name}`);
    expect(actions).not.toContain('Install Codex');
    expect(actions).not.toContain('New Amp session');
    expect(await rendered(false, MOCK_AGENTS)).not.toContain('Install Amp');
  });
});
