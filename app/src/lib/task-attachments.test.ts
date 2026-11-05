import { describe, expect, it, vi } from 'vitest';
import { uploadBlobs, uploadPaths } from './task-attachments';
import { MAX_UPLOAD } from './uploads';

const blob = (name: string, size = 3) => ({ name, size, arrayBuffer: async () => new ArrayBuffer(size) });

function client(fails: string[] = []) {
  return {
    savePaste: vi.fn(async (name: string) => `/tmp/paste/${name}`),
    uploadFile: vi.fn(async (path: string) => {
      if (fails.some((f) => path.endsWith(f))) throw new Error('daemon down');
      return { path: `/home/u/.overseer/u/${path.split('/').pop()}`, name: 'x', size: 1 };
    }),
  };
}

describe('uploadBlobs', () => {
  it('saves each blob locally then uploads it, keeping the original name', async () => {
    const c = client();
    const fail = vi.fn();
    const out = await uploadBlobs(c, [blob('shot.png'), blob('')], fail);
    expect(out).toEqual([
      { name: 'shot.png', path: '/home/u/.overseer/u/shot.png' },
      { name: 'screenshot.png', path: '/home/u/.overseer/u/screenshot.png' },
    ]);
    expect(c.savePaste).toHaveBeenCalledWith('shot.png', expect.any(Uint8Array));
    expect(fail).not.toHaveBeenCalled();
  });

  it('refuses files over 50 MB without uploading them', async () => {
    const c = client();
    const fail = vi.fn();
    const out = await uploadBlobs(c, [blob('big.mp4', MAX_UPLOAD + 1), blob('ok.txt')], fail);
    expect(out.map((a) => a.name)).toEqual(['ok.txt']);
    expect(fail).toHaveBeenCalledWith({ name: 'big.mp4', error: '> 50 MB' });
    expect(c.savePaste).toHaveBeenCalledTimes(1);
  });

  it('reports a failed upload and keeps the others', async () => {
    const fail = vi.fn();
    const out = await uploadBlobs(client(['bad.png']), [blob('bad.png'), blob('good.png')], fail);
    expect(out.map((a) => a.name)).toEqual(['good.png']);
    expect(fail).toHaveBeenCalledWith({ name: 'bad.png', error: 'daemon down' });
  });
});

describe('uploadPaths', () => {
  it('uploads dropped paths and names them by their basename', async () => {
    const fail = vi.fn();
    const out = await uploadPaths(client(['b.pdf']), ['/Users/me/a.png', 'C:\\docs\\b.pdf'], fail);
    expect(out).toEqual([{ name: 'a.png', path: '/home/u/.overseer/u/a.png' }]);
    expect(fail).toHaveBeenCalledWith({ name: 'b.pdf', error: 'daemon down' });
  });
});
