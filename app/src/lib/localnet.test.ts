import { describe, expect, it } from 'vitest';
import { diagnostics, isNoRoute, localNetworkView, terminalCommand, unstableIdentity } from './localnet';
import type { LocalNetworkReport } from './types';

const NO_ROUTE = 'ssh: connect to host workstation port 22: No route to host';
const report = (r: Partial<LocalNetworkReport>): LocalNetworkReport => ({ status: 'granted', addresses: [], private: true, ...r });

describe('local network', () => {
  it('recognizes no-route errors', () => {
    expect(isNoRoute(NO_ROUTE)).toBe(true);
    expect(isNoRoute('connect to host workstation (192.168.1.5) port 22: No route to host (os error 65)')).toBe(true);
    expect(isNoRoute('connect: EHOSTUNREACH')).toBe(true);
    expect(isNoRoute('Permission denied (publickey).')).toBe(false);
    expect(isNoRoute('os error 650')).toBe(false);
    expect(isNoRoute(undefined)).toBe(false);
  });

  it('picks the dialog from the probe and the error', () => {
    expect(localNetworkView(null, NO_ROUTE)).toBe('none');
    expect(localNetworkView(report({ status: 'notApplicable' }), NO_ROUTE)).toBe('none');
    expect(localNetworkView(report({ status: 'denied' }), 'Connection refused')).toBe('permission');
    expect(localNetworkView(report({ status: 'denied' }), NO_ROUTE)).toBe('permission');
    expect(localNetworkView(report({ status: 'unknown' }), NO_ROUTE)).toBe('permission');
    expect(localNetworkView(report({ status: 'unknown' }), 'timeout')).toBe('none');
    expect(localNetworkView(report({ status: 'unreachable' }), NO_ROUTE)).toBe('unreachable');
    expect(localNetworkView(report({ status: 'granted' }), NO_ROUTE)).toBe('unreachable');
    expect(localNetworkView(report({ status: 'granted' }), 'Permission denied (publickey).')).toBe('none');
  });

  it('flags builds whose identity changes on every update', () => {
    expect(unstableIdentity(report({ signing: 'adhoc' }))).toBe(true);
    expect(unstableIdentity(report({ signing: 'unsigned' }))).toBe(true);
    expect(unstableIdentity(report({ signing: 'developerId' }))).toBe(false);
    expect(unstableIdentity(report({ signing: 'selfSigned' }))).toBe(false);
    expect(unstableIdentity(null)).toBe(false);
  });

  it('lists only the diagnostics that are known', () => {
    const rows = diagnostics(
      report({ endpoint: 'workstation:22', addresses: ['192.168.1.5', 'fe80::1'], interface: 'en0', connect: 'No route to host (os error 65)', signing: 'adhoc', via: null }),
    );
    expect(rows).toEqual([
      ['endpoint', 'workstation:22'],
      ['addresses', '192.168.1.5, fe80::1'],
      ['interface', 'en0'],
      ['connect', 'No route to host (os error 65)'],
      ['signing', 'adhoc'],
    ]);
  });

  it('builds the Terminal workaround command from the bundle', () => {
    expect(terminalCommand(report({ bundle: '/Applications/Overseer.app' }))).toBe("'/Applications/Overseer.app/Contents/MacOS/overseer-app'");
    expect(terminalCommand(report({ bundle: "/Users/o'neil/Apps/Overseer.app/" }))).toBe("'/Users/o'\\''neil/Apps/Overseer.app/Contents/MacOS/overseer-app'");
    expect(terminalCommand(report({ bundle: '/usr/local/bin/overseer-app' }))).toBe("'/usr/local/bin/overseer-app'");
    expect(terminalCommand(report({ bundle: null }))).toBeNull();
    expect(terminalCommand(null)).toBeNull();
  });
});
