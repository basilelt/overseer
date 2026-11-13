import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { app } from '../../state.svelte';
import type { AgentInfo } from '../../types';
import NewSessionMenu from './NewSessionMenu.svelte';
import NewSessionList from './NewSessionList.svelte';

const agent = (id: string, over: Partial<AgentInfo> = {}): AgentInfo => ({ id, name: id.toUpperCase(), installed: true, efforts: [], resume: false, verified: true, sessions: true, turns: true, ...over });
const before = { agents: app.agents, connected: app.connected };
afterEach(() => Object.assign(app, before));

const list = () => render(NewSessionList, { props: { onclose: () => {} } }).body;

describe('new session menu button', () => {
  it('is the tour target, a menu button that starts closed', () => {
    app.connected = true;
    const h = render(NewSessionMenu).body;
    expect(h).toContain('data-tour="new-session"');
    expect(h).toContain('aria-haspopup="menu"');
    expect(h).toContain('aria-expanded="false"');
    expect(h).not.toMatch(/<button[^>]*\bdisabled/);
  });

  it('is disabled while the daemon is not connected', () => {
    app.connected = false;
    expect(render(NewSessionMenu).body).toMatch(/<button[^>]*\bdisabled/);
  });

  it('renders no menu list while closed (the lazy chunk is held by the startup bundle budget test)', () => {
    app.connected = true;
    expect(render(NewSessionMenu).body).not.toContain('role="menu"');
  });
});

describe('new session list', () => {
  it('lists the installed agents with their version as the tooltip', () => {
    app.agents = [agent('claude', { version: '2.1.0' }), agent('codex')];
    const h = list();
    expect(h).toContain('role="menu"');
    expect(h.match(/role="menuitem"/g)).toHaveLength(2);
    expect(h).toContain('title="2.1.0"');
    expect(h).toContain('>CLAUDE<');
    expect(h).toContain('>2.1.0<');
    expect(h).toContain('title="CODEX"');
  });

  it('folds the missing agents behind one collapsed row that counts them', () => {
    app.agents = [agent('claude'), agent('aider', { installed: false, install: 'pip install aider' }), agent('gemini', { installed: false })];
    const h = list();
    expect(h).toContain('>CLAUDE<');
    expect(h).toContain('aria-expanded="false"');
    expect(h).not.toContain('AIDER');
    expect(h).not.toContain('GEMINI');
    expect(h.match(/role="menuitem"/g)).toHaveLength(2);
  });

  it('shows no fold row when every agent is installed', () => {
    app.agents = [agent('claude')];
    expect(list()).not.toContain('aria-expanded');
  });
});
