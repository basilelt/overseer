import { describe, expect, it, vi } from 'vitest';
import { api } from './daemon/api';
import type { DaemonClient } from './daemon/client';
import { DaemonError } from './daemon/client';
import { MockDaemon } from './daemon/mock';
import { openFile } from './preview';
import { app } from './state.svelte';
import { quietMock } from '../test/quiet-mock';

describe('openFile', () => {
  it('opens the file overlay with the path and term', () => {
    openFile('/tmp/a b.png', 't1');
    expect(app.overlay).toBe('file');
    expect(app.fileArg).toEqual({ path: '/tmp/a b.png', term: 't1' });
    expect(app.overlayArg).toBeNull();
    openFile('/tmp/x.md');
    expect(app.fileArg).toEqual({ path: '/tmp/x.md' });
    openFile('docs/a.pdf', undefined, 'l1');
    expect(app.fileArg).toEqual({ path: 'docs/a.pdf', line: 'l1' });
    app.open('profiles', 'global');
    expect(app.fileArg).toBeNull();
    expect(app.overlayArg).toBe('global');
    app.open(null);
  });
});

describe('fileBytes', () => {
  it('builds the route with an encoded path and optional term', async () => {
    const fetchBytes = vi.fn(async () => new ArrayBuffer(0));
    const a = api({ fetchBytes } as unknown as DaemonClient);
    await a.fileBytes({ path: '~/dir/a b&c.png', term: 't 1' });
    await a.fileBytes({ path: '/x.txt' });
    await a.fileBytes({ path: 'd/a.pdf', line: 'host-01AB' });
    expect(fetchBytes.mock.calls).toEqual([['/v1/file?path=%7E%2Fdir%2Fa+b%26c.png&term=t+1'], ['/v1/file?path=%2Fx.txt'], ['/v1/file?path=d%2Fa.pdf&line=host-01AB']]);
  });
});

describe('MockDaemon.fetchBytes', () => {
  const a = api(quietMock());
  const text = async (p: string) => new TextDecoder().decode(await a.fileBytes({ path: p }));

  it('serves bytes for images, markdown and text', async () => {
    expect(await text('/tmp/shot.svg')).toMatch(/^<svg[\s\S]*shot\.svg/);
    expect([...new Uint8Array(await a.fileBytes({ path: '/tmp/shot.png' })).slice(1, 4)]).toEqual([0x50, 0x4e, 0x47]);
    expect(await text('/tmp/README.md')).toMatch(/^# README\.md/);
    expect(await text('/tmp/run.log')).toContain('demo line for /tmp/run.log');
  });

  it('rejects files not in the demo', async () => {
    await expect(a.fileBytes({ path: '/tmp/clip.mp4' })).rejects.toBeInstanceOf(DaemonError);
    await expect(a.fileBytes({ path: '/tmp/doc.pdf' })).rejects.toMatchObject({ status: 404 });
  });

  it('resolves relative paths only against a term or a line, like the daemon', async () => {
    await expect(a.fileBytes({ path: 'docs/a.md' })).rejects.toMatchObject({ status: 400, message: 'relative path needs a known term' });
    expect(new TextDecoder().decode(await a.fileBytes({ path: 'docs/a.md', line: 'l1' }))).toMatch(/^# a\.md/);
    expect(new TextDecoder().decode(await a.fileBytes({ path: 'docs/a.md', term: 't1' }))).toMatch(/^# a\.md/);
  });
});
