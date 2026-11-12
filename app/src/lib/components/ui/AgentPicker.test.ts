import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import type { AgentInfo } from '../../types';
import AgentPicker from './AgentPicker.svelte';

const agent = (id: string, over: Partial<AgentInfo> = {}): AgentInfo => ({ id, name: id.toUpperCase(), installed: true, efforts: [], resume: false, verified: true, sessions: true, turns: true, ...over });
const absent = (id: string, installable: boolean) => agent(id, { installed: false, installable, install: installable ? 'npm i -g x' : undefined });

const html = (agents: AgentInfo[]) => render(AgentPicker, { props: { agents, value: 'claude', label: 'Agent', onchange: () => {}, open: true } }).body;
beforeEach(() => vi.stubGlobal('innerHeight', 800));
afterEach(() => vi.unstubAllGlobals());

describe('agent picker options', () => {
  it('greys out an absent agent that cannot be installed, but not an installable one', () => {
    const h = html([agent('claude'), absent('codex', true), absent('aider', false)]);
    const li = (name: string) => h.match(new RegExp(`<li[^>]*>(?:(?!</li>)[\\s\\S])*>${name}<`))?.[0] ?? '';
    expect(li('CODEX')).toContain('aria-disabled="false"');
    expect(li('CODEX')).toContain('Install');
    expect(li('AIDER')).toContain('aria-disabled="true"');
    expect(li('CLAUDE')).toContain('aria-disabled="false"');
    expect(li('CLAUDE')).toContain('aria-selected="true"');
  });

  it('offers nothing to pick while closed', () => {
    const h = render(AgentPicker, { props: { agents: [agent('claude')], value: 'claude', label: 'Agent', onchange: () => {} } }).body;
    expect(h).toContain('aria-expanded="false"');
    expect(h).not.toContain('role="listbox"');
  });
});
