import { describe, expect, it } from 'vitest';
import libRs from '../../src-tauri/src/lib.rs?raw';
import connRs from '../../src-tauri/src/conn.rs?raw';
import sshRs from '../../src-tauri/src/ssh.rs?raw';
import typesTs from './types.ts?raw';
import { CMD, PLUGIN_CMD } from './tauri-commands';

const NOT_CALLED_BY_FRONTEND: string[] = [];

function handlerLists(src: string): string[][] {
  return [...src.matchAll(/generate_handler!\[([\s\S]*?)\]\)?;/g)].map((m) => [...m[1].matchAll(/^\s*(?:[a-z_]+::)+(\w+),/gm)].map((n) => n[1]));
}

const lists = handlerLists(libRs);
const [desktop, mobile] = lists;
const registered = new Set(lists.flat());
const table = new Set<string>(Object.values(CMD));

describe('Tauri command table', () => {
  it('parses the desktop and the mobile handler lists', () => {
    expect(lists).toHaveLength(2);
    expect(desktop).toContain('daemon_request');
    expect(mobile).toContain('mobile_minimize');
  });

  it('has unique names', () => {
    expect(table.size).toBe(Object.keys(CMD).length);
  });

  it('only names commands registered in a handler list', () => {
    expect([...table].filter((n) => !registered.has(n))).toEqual([]);
  });

  it('names every registered command the frontend calls', () => {
    expect([...registered].filter((n) => !table.has(n) && !NOT_CALLED_BY_FRONTEND.includes(n)).sort()).toEqual([]);
  });

  it('lists only registered commands as not called', () => {
    expect(NOT_CALLED_BY_FRONTEND.filter((n) => !registered.has(n) || table.has(n))).toEqual([]);
  });

  it('keeps plugin commands out of the handler table', () => {
    for (const n of Object.values(PLUGIN_CMD)) expect(n.startsWith('plugin:')).toBe(true);
  });

  it('knows which commands exist on one platform only', () => {
    const desktopOnly = [...table].filter((n) => desktop.includes(n) && !mobile.includes(n));
    const mobileOnly = [...table].filter((n) => mobile.includes(n) && !desktop.includes(n));
    expect(desktopOnly).toContain('ssh_hosts');
    expect(desktopOnly).toContain('browser_open');
    expect(mobileOnly).toContain('mobile_minimize');
    expect(mobileOnly).toContain('apk_update_status');
  });
});

describe('connection phases', () => {
  it('emits only phases the frontend knows', () => {
    const union = /export type ConnPhase =([^;]*);/.exec(typesTs)![1];
    const known = [...union.matchAll(/'(\w+)'/g)].map((m) => m[1]);
    const emitted = new Set<string>();
    for (const src of [connRs, sshRs]) {
      for (const m of src.matchAll(/(?:ConnState::new\(|step\(rep, target, |on_step\(|phase: )"(\w+)"/g)) emitted.add(m[1]);
    }
    expect(emitted.size).toBeGreaterThan(5);
    expect([...emitted].filter((p) => !known.includes(p))).toEqual([]);
  });
});
