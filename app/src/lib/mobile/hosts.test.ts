import { describe, expect, it } from 'vitest';
import { connectHint, describeHost, emptyForm, formatDest, formFromHost, hostFromForm, parseDest } from './hosts';
import type { SavedHost } from '../types';

describe('saved host form', () => {
  it('parses user@host:port like the Rust side', () => {
    expect(parseDest('alice@box.lan')).toEqual({ host: 'box.lan', port: 22, user: 'alice' });
    expect(parseDest(' box:2222 ')).toEqual({ host: 'box', port: 2222, user: '' });
    expect(parseDest('u@[fd00::2]:2200')).toEqual({ host: 'fd00::2', port: 2200, user: 'u' });
    expect(parseDest('fd00::2')).toEqual({ host: 'fd00::2', port: 22, user: '' });
    expect(parseDest('box:x')).toBeNull();
    expect(parseDest('u@')).toBeNull();
    expect(parseDest('two words')).toBeNull();
    expect(parseDest('box:70000')).toBeNull();
  });

  it('formats endpoints back', () => {
    expect(formatDest({ host: 'box', port: 22, user: 'u' })).toBe('u@box');
    expect(formatDest({ host: 'fd00::2', port: 2200, user: '' })).toBe('[fd00::2]:2200');
  });

  it('builds a saved host and reports the first invalid field', () => {
    const f = { ...emptyForm(), host: ' box.lan ', user: 'alice', jump: 'bastion.example.com:2222' };
    const r = hostFromForm(f);
    expect(r).toEqual({
      host: {
        id: '',
        label: 'box.lan',
        host: 'box.lan',
        port: 22,
        user: 'alice',
        jump: { host: 'bastion.example.com', port: 2222, user: 'alice' },
        auth: 'key',
        rememberPassword: false,
      },
    });
    expect(hostFromForm({ ...emptyForm(), user: 'u' })).toEqual({ error: 'host' });
    expect(hostFromForm({ ...emptyForm(), host: 'box' })).toEqual({ error: 'user' });
    expect(hostFromForm({ ...emptyForm(), host: 'box', user: 'u', port: '0' })).toEqual({ error: 'port' });
    expect(hostFromForm({ ...emptyForm(), host: 'box', user: 'u', jump: 'a b' })).toEqual({ error: 'jump' });
    const pw = hostFromForm({ ...emptyForm(), host: 'box', user: 'u', auth: 'password', rememberPassword: true });
    expect('host' in pw && pw.host.rememberPassword).toBe(true);
    const key = hostFromForm({ ...emptyForm(), host: 'box', user: 'u', rememberPassword: true });
    expect('host' in key && key.host.rememberPassword).toBe(false);
  });

  it('round-trips through the form', () => {
    const h: SavedHost = { id: 'h2', label: 'GPU', host: '10.0.4.30', port: 2200, user: 'b', jump: { host: 'bastion', port: 22, user: 'b' }, auth: 'key', rememberPassword: false };
    expect(formFromHost(h).jump).toBe('b@bastion');
    expect(hostFromForm(formFromHost(h))).toEqual({ host: h });
    expect(describeHost(h)).toBe('b@10.0.4.30:2200 via b@bastion');
  });
});

describe('connect hints', () => {
  it('classifies errors the phone can explain', () => {
    expect(connectHint('the overseer server is not installed on this host (linux-x86_64): cannot download the server from GitHub: dns')).toBe('notInstalled');
    expect(connectHint("Permission denied for u@box:22: the server did not accept this device's key. Add…")).toBe('deviceKey');
    expect(connectHint('REMOTE HOST IDENTIFICATION HAS CHANGED for box:22')).toBe('hostKeyChanged');
    expect(connectHint('ssh: connect to host box port 22: Connection timed out')).toBe('unreachable');
    expect(connectHint('something else')).toBeNull();
    expect(connectHint(undefined)).toBeNull();
  });
});
