import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { MOCK_AGENTS } from '../../daemon/mock';
import type { AgentInfo } from '../../types';

async function rendered(connected: boolean, current?: Record<string, unknown>, all: Record<string, unknown>[] = [], agents: AgentInfo[] = [], projects = true) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  vi.spyOn(app, 'connected', 'get').mockReturnValue(connected);
  vi.spyOn(app, 'projects', 'get').mockReturnValue(projects ? ([{ path: '/p' }] as never) : []);
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

beforeAll(async () => {
  await Promise.all([import('svelte/server'), import('./CommandPalette.svelte')]);
}, 60_000);

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
    expect(views).toContain('Open browser');
    expect(views).not.toContain('Open the worktrees');
    expect(views).not.toContain('Open Git (graph and source control)');
    expect(views).not.toContain('Review changes');
  });

  it.each(['Open Git (graph and source control)', 'Open the worktrees'])('lists "%s" only when connected with a project, like the phone More sheet', async (entry) => {
    expect(section(await rendered(true), 'Views')).toContain(entry);
    expect(section(await rendered(true, undefined, [], [], false), 'Views')).not.toContain(entry);
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

  it('shows the shortcut of a listed command, but none for source control', async () => {
    const body = await rendered(true);
    const row = (label: string) => body.match(new RegExp(`<li[^>]*role="option"[^>]*>(?:(?!</li>)[\\s\\S])*${label}(?:(?!</li>)[\\s\\S])*</li>`))?.[0] ?? '';
    expect(row('Open tasks board')).toContain('Ctrl+Shift+K');
    expect(row('Toggle sidebar')).toContain('Ctrl+B');
    expect(row('Show or hide source control in the side panel')).not.toContain('class="keys');
    expect(row('Show or hide source control in the side panel')).not.toBe('');
  });

  it('keeps the entries of a disconnected palette to the ones that need no host', async () => {
    const body = await rendered(false);
    for (const label of ['Open browser', 'Toggle theme', 'Show logs', 'Keyboard shortcuts']) expect(body).toContain(label);
    for (const label of ['Disconnect', 'Ports: show forwarded ports', 'Organize my sessions', 'New workspace from a folder']) expect(body).not.toContain(label);
  });
});
