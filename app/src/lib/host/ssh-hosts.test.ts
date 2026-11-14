import { describe, expect, it, vi } from 'vitest';
import { loadSshHosts } from './ssh-hosts';

const hosts = [{ alias: 'box' }];

describe('loadSshHosts', () => {
  it('returns the hosts of the ssh config on desktop', async () => {
    const sshHosts = vi.fn(async () => hosts);
    expect(await loadSshHosts({ sshHosts }, false)).toEqual(hosts);
  });

  it('never calls the desktop-only command on mobile', async () => {
    const sshHosts = vi.fn(async () => hosts);
    expect(await loadSshHosts({ sshHosts }, true)).toEqual([]);
    expect(sshHosts).not.toHaveBeenCalled();
  });

  it('returns nothing without a client', async () => {
    expect(await loadSshHosts(null, false)).toEqual([]);
    expect(await loadSshHosts(undefined, false)).toEqual([]);
  });

  it('returns nothing when the command rejects', async () => {
    expect(await loadSshHosts({ sshHosts: async () => Promise.reject(new Error('no ssh config')) }, false)).toEqual([]);
  });
});
