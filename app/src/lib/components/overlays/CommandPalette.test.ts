import { afterEach, describe, expect, it, vi } from 'vitest';
import { MOCK_AGENTS } from '../../mock';
import type { AgentInfo } from '../../types';

async function rendered(connected: boolean, current?: Record<string, unknown>, all: Record<string, unknown>[] = [], agents: AgentInfo[] = []) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  vi.spyOn(app, 'connected', 'get').mockReturnValue(connected);
  if (current) {
    vi.spyOn(app, 'currentWorkspace', 'get').mockReturnValue(current as never);
    vi.spyOn(app, 'wsPersistent', 'get').mockReturnValue(true);
    app.ws = { workspaces: all as never, folders: [], groups: [] };
  }
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
    for (const label of ['Review changes', 'Open browser', 'Open Git (graph and source control)', 'Open the worktrees', 'Open PR reviews']) expect(views).toContain(label);
  });

  it('keeps the views out of the Actions section', async () => {
    const actions = section(await rendered(true), 'Actions');
    expect(actions).not.toContain('Open Git (graph and source control)');
    expect(actions).not.toContain('Open PR reviews');
  });

  it('still lists the local views when disconnected', async () => {
    const views = section(await rendered(false), 'Views');
    expect(views).toContain('Open Git (graph and source control)');
    expect(views).not.toContain('Review changes');
  });

  it('lists Install skills and plugins under Actions only when connected', async () => {
    expect(section(await rendered(true), 'Actions')).toContain('Install skills and plugins');
    expect(await rendered(false)).not.toContain('Install skills and plugins');
  });

  it('offers a new subject for a folder workspace or the parent of a nested one, never for a subject or none', async () => {
    const parent = { id: 'p', name: 'endurance', path: '/e', kind: 'dir', parentId: null };
    const nested = { id: 'n', name: 'Onboarding', path: '/e/x', kind: 'dir', parentId: 'p' };
    const label = 'New subject in the current workspace';
    expect(section(await rendered(true, parent, [parent]), 'Actions')).toContain(label);
    expect(section(await rendered(true, nested, [parent, nested]), 'Actions')).toContain(label);
    expect(await rendered(true, { id: 's', name: 'S', path: null, kind: 'subject', parentId: null }, [])).not.toContain(label);
    expect(await rendered(true)).not.toContain(label);
  });

  it('offers to install each absent agent that has an install command', async () => {
    const actions = section(await rendered(true, undefined, [], MOCK_AGENTS), 'Actions');
    for (const name of ['opencode', 'Amp', 'Cursor Agent']) expect(actions).toContain(`Install ${name}`);
    expect(actions).not.toContain('Install Codex');
    expect(actions).not.toContain('New Amp session');
    expect(await rendered(false, undefined, [], MOCK_AGENTS)).not.toContain('Install Amp');
  });
});
