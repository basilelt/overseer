import { afterEach, describe, expect, it, vi } from 'vitest';

async function rendered(connected: boolean) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  vi.spyOn(app, 'connected', 'get').mockReturnValue(connected);
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
});
