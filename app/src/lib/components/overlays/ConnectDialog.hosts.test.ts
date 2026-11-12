import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { settings } from '../../settings/settings.svelte';
import { app } from '../../state.svelte';
import ConnectDialog from './ConnectDialog.svelte';
import src from './ConnectDialog.svelte?raw';

const prod = { kind: 'ssh' as const, host: 'prod', label: 'prod' };
const html = () => render(ConnectDialog, { props: { onclose: () => {} } }).body;

describe('ConnectDialog: where a host opens', () => {
  const before = { overlayArg: app.overlayArg, hostWindows: settings.hostWindows };
  afterEach(() => {
    app.overlayArg = before.overlayArg;
    settings.hostWindows = before.hostWindows;
  });

  it('asks where the host opens, the mode picked in Settings first', () => {
    app.overlayArg = `ask:${JSON.stringify(prod)}`;
    settings.hostWindows = 'shared';
    const out = html();
    expect(out).toContain('Where should prod open?');
    expect(out).toContain('Remember for prod');
    expect(out).toMatch(/Open in a new window<\/button>.*class="btn primary[^"]*"[^>]*>Add to this window/s);
  });

  it('puts "Open in a new window" first in one-window-per-host mode', () => {
    app.overlayArg = `ask:${JSON.stringify(prod)}`;
    settings.hostWindows = 'separate';
    expect(html()).toMatch(/Add to this window<\/button>.*class="btn primary[^"]*"[^>]*>Open in a new window/s);
  });

  it('shows the host list when nothing is pending, or when the pending host is not valid', () => {
    app.overlayArg = 'ask:{"kind":"ssh"}';
    expect(html()).not.toContain('Where should');
    app.overlayArg = 'ask:{bad';
    expect(html()).not.toContain('Where should');
  });

  it('decides with connectHow and remembers the answer per host only when asked to', () => {
    expect(src).toContain("connectHow(settings, target, { active: hostKey(app.conn.target), keys: app.hosts.map((l) => l.key), live: app.connected }, forced)");
    expect(src).toMatch(/if \(remember\) saveSettings\(\{ hostChoice: \{ \.\.\.settings\.hostChoice, \[hostKey\(target\)\]: c \} \}\);/);
    expect(src).toContain("app.connect(target, how === 'add' ? 'add' : 'replace')");
    expect(src).toContain("app.overlayArg === 'add' ? 'add' : undefined");
  });

  it('follows only the host being connected, not the one the main area showed before', () => {
    expect(src).toContain('const mine = $derived(!!lastTarget && hostKey(app.conn.target) === hostKey(lastTarget));');
    expect(src).toContain('if (started && mine && !reached.includes(p))');
    expect(src).toContain("if (!started || !mine || app.conn.phase !== 'connected') return;");
  });
});
