import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import MobileMore, { tools } from './MobileMore.svelte';
import { command } from '../../commands/commands';
import { app } from '../../state.svelte';

afterEach(() => vi.restoreAllMocks());

function body(connected: boolean, projects = true): string {
  vi.spyOn(app, 'connected', 'get').mockReturnValue(connected);
  vi.spyOn(app, 'projects', 'get').mockReturnValue(projects ? ([{ path: '/p' }] as never) : []);
  return render(MobileMore, { props: {} }).body;
}

const has = (html: string, label: string) => new RegExp(`class="label[^"]*">${label}<`).test(html);
const rows = (html: string) => (html.match(/<button class="row/g) ?? []).length;

describe('MobileMore availability', () => {
  it('lists exactly the tools whose command is available, never a greyed row', () => {
    for (const connected of [false, true]) {
      const html = body(connected);
      expect(html).not.toContain('disabled');
      for (const tool of tools) expect(has(html, tool.label()), `${tool.id} connected=${connected}`).toBe(command(tool.id).available());
    }
  });

  it('hides the connection tools and the host-only rows while disconnected', () => {
    const html = body(false);
    for (const tool of tools.filter((x) => ['ports', 'system', 'graph'].includes(x.id))) expect(has(html, tool.label()), tool.id).toBe(false);
    expect(has(html, 'Disconnect')).toBe(false);
    expect(has(html, 'Settings')).toBe(true);
  });

  it('shows more rows when connected, and the Git graph only with a project', () => {
    expect(rows(body(true))).toBeGreaterThan(rows(body(false)));
    const graph = tools.find((x) => x.id === 'graph')!.label();
    expect(has(body(true, false), graph)).toBe(false);
    expect(has(body(true, true), graph)).toBe(true);
  });

  it('keeps no empty section header', () => {
    const html = body(false);
    for (const h of html.split('<h3>').slice(1)) expect(h.split('</h3>')[1].trim().length).toBeGreaterThan(0);
  });
});
