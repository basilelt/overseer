import { describe, expect, it, vi } from 'vitest';
import { attachImages, type ComposeAttachDeps } from './compose-attach';

const png = (name: string, size = 1) => new File([new Uint8Array(size)], name, { type: 'image/png' });

function deps(over: Partial<{ savePaste: (n: string, b: Uint8Array) => Promise<string>; uploadFile: (p: string) => Promise<{ name: string; path: string; size: number }>; clip: string | null }> = {}) {
  const notify = vi.fn();
  const client = {
    savePaste: vi.fn(over.savePaste ?? (async (n: string) => `/tmp/${n}`)),
    uploadFile: vi.fn(over.uploadFile ?? (async (p: string) => ({ name: p, path: `/up${p}`, size: 1 }))),
  };
  const clipboardImage = vi.fn(async () => over.clip ?? null);
  return { notify, client, clipboardImage, all: { client, clipboardImage, notify } as unknown as ComposeAttachDeps };
}

describe('attachImages', () => {
  it('saves each image then uploads it and returns the host paths', async () => {
    const d = deps();
    expect(await attachImages([png('a.png'), png('b.png')], d.all)).toEqual(['/up/tmp/a.png', '/up/tmp/b.png']);
    expect(d.client.savePaste).toHaveBeenCalledTimes(2);
    expect(d.notify).not.toHaveBeenCalled();
  });

  it('refuses a file over the size limit with the shared message and uploads nothing', async () => {
    const d = deps();
    const big = { name: 'huge.png', type: 'image/png', size: 60 * 1024 * 1024, arrayBuffer: async () => new ArrayBuffer(0) } as unknown as File;
    expect(await attachImages([big], d.all)).toEqual([]);
    expect(d.notify).toHaveBeenCalledWith('huge.png is over the 50 MB limit', 'error');
    expect(d.client.savePaste).not.toHaveBeenCalled();
  });

  it('reports a failed save once, by name, and stops', async () => {
    const d = deps({ savePaste: async () => { throw new Error('disk full'); } });
    expect(await attachImages([png('a.png'), png('b.png')], d.all)).toEqual([]);
    expect(d.notify).toHaveBeenCalledOnce();
    expect(d.notify).toHaveBeenCalledWith("Couldn't paste a.png: disk full", 'error');
  });

  it('shows one summary notice when several uploads fail, and keeps the ones that worked', async () => {
    const d = deps({
      uploadFile: async (p) => {
        if (!p.endsWith('a.png')) throw new Error('host down');
        return { name: p, path: `/up${p}`, size: 1 };
      },
    });
    expect(await attachImages([png('a.png'), png('b.png'), png('c.png')], d.all)).toEqual(['/up/tmp/a.png']);
    expect(d.notify).toHaveBeenCalledOnce();
    expect(d.notify).toHaveBeenCalledWith("Couldn't upload 2 files, last error: host down", 'error', undefined, 'upload');
  });

  it('names the file in the notice when a single upload fails', async () => {
    const d = deps({ uploadFile: async () => { throw new Error('host down'); } });
    expect(await attachImages([png('a.png')], d.all)).toEqual([]);
    expect(d.notify).toHaveBeenCalledWith("Couldn't upload a.png: host down", 'error', undefined, 'upload');
  });

  it('treats an upload answer without a path as a failure', async () => {
    const d = deps({ uploadFile: async () => ({ name: 'a', path: '', size: 1 }) });
    expect(await attachImages([png('a.png')], d.all)).toEqual([]);
    expect(d.notify).toHaveBeenCalledWith("Couldn't upload a.png: the daemon returned no file path", 'error', undefined, 'upload');
  });

  it('falls back to the native clipboard image when the paste has no file', async () => {
    const d = deps({ clip: '/data/clip.png' });
    expect(await attachImages([], d.all)).toEqual(['/up/data/clip.png']);
    expect(d.client.savePaste).not.toHaveBeenCalled();
  });

  it('returns nothing when there is neither a file nor a native image', async () => {
    const d = deps();
    expect(await attachImages([], d.all)).toEqual([]);
    expect(d.client.uploadFile).not.toHaveBeenCalled();
  });
});
