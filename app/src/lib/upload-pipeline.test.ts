import { describe, expect, it, vi } from 'vitest';
import type { UploadResult } from './types';
import { savePasteAll, uploadFailures, uploadList } from './upload-pipeline';
import { clipboardFiles, fileInfos, PROGRESS_FROM, refusalMessage } from './uploads';

const file = (name: string, body = 'x') => new File([body], name, { type: 'image/png' });
const result = (name: string): UploadResult => ({ name, path: `/up/${name}`, size: 1 });

describe('refusalMessage', () => {
  it('names the file that is too large or a folder', () => {
    expect(refusalMessage({ kind: 'reject', reason: 'tooLarge', name: 'a.bin' })).toBe('a.bin is over the 50 MB limit');
    expect(refusalMessage({ kind: 'reject', reason: 'folder', name: 'src' })).toBe('src is a folder; only files can be sent to a remote host');
  });
});

describe('fileInfos', () => {
  it('keeps the name, type and size of each file', () => {
    expect(fileInfos([file('a.png', 'abc')])).toEqual([{ name: 'a.png', type: 'image/png', size: 3 }]);
  });
});

describe('clipboardFiles', () => {
  it('prefers the files of the clipboard', () => {
    const a = file('a.png');
    expect(clipboardFiles({ files: [a] as unknown as FileList, items: [] as unknown as DataTransferItemList })).toEqual([a]);
  });

  it('falls back to file items and skips text items', () => {
    const a = file('a.png');
    const items = [{ kind: 'string', getAsFile: () => null }, { kind: 'file', getAsFile: () => a }, { kind: 'file', getAsFile: () => null }] as unknown as DataTransferItemList;
    expect(clipboardFiles({ files: [] as unknown as FileList, items })).toEqual([a]);
  });
});

describe('savePasteAll', () => {
  it('saves every file under its name, naming anonymous ones paste.png', async () => {
    const save = vi.fn(async (name: string, _bytes: Uint8Array) => `/tmp/${name}`);
    const out = await savePasteAll([file('a.png'), new File(['y'], '')], save, vi.fn());
    expect(out).toEqual(['/tmp/a.png', '/tmp/paste.png']);
    expect(save.mock.calls[0][1]).toBeInstanceOf(Uint8Array);
  });

  it('stops at the first failure and reports it', async () => {
    const save = vi.fn(async (name: string) => {
      if (name === 'b.png') throw new Error('disk full');
      return `/tmp/${name}`;
    });
    const fail = vi.fn();
    expect(await savePasteAll([file('a.png'), file('b.png'), file('c.png')], save, fail)).toBeNull();
    expect(fail).toHaveBeenCalledOnce();
    expect(fail.mock.calls[0][0]).toBe('b.png');
    expect(save).toHaveBeenCalledTimes(2);
  });
});

describe('uploadList', () => {
  const hooks = () => ({ progress: vi.fn(), fail: vi.fn(), done: vi.fn(), validate: vi.fn() });

  it('uploads in order and returns the results', async () => {
    const h = hooks();
    const out = await uploadList(async (p) => result(p.split('/').pop()!), ['/a/1.png', '/a/2.png'], h);
    expect(out.map((r) => r.name)).toEqual(['1.png', '2.png']);
    expect(h.done).toHaveBeenCalledTimes(2);
    expect(h.done.mock.calls[0][2]).toBe(false);
    expect(h.fail).not.toHaveBeenCalled();
  });

  it('reports progress only for large files and tells done about it', async () => {
    const h = hooks();
    await uploadList(
      async (_p, onProgress) => {
        onProgress({ sent: 1, total: PROGRESS_FROM });
        onProgress({ sent: 1, total: PROGRESS_FROM * 2 });
        return result('big.bin');
      },
      ['/a/big.bin'],
      h,
    );
    expect(h.progress).toHaveBeenCalledOnce();
    expect(h.progress).toHaveBeenCalledWith('big.bin', 1 / (PROGRESS_FROM * 2));
    expect(h.done.mock.calls[0][2]).toBe(true);
  });

  it('reports a failing upload and carries on with the next one', async () => {
    const h = hooks();
    const upload = vi.fn(async (p: string) => {
      if (p === '/a/bad.png') throw new Error('refused');
      return result('ok.png');
    });
    const out = await uploadList(upload, ['/a/bad.png', '/a/ok.png'], h);
    expect(out).toHaveLength(1);
    expect(h.fail).toHaveBeenCalledWith('bad.png', expect.objectContaining({ message: 'refused' }));
  });

  it('treats a result the validator rejects as a failure', async () => {
    const h = hooks();
    h.validate.mockImplementation(() => {
      throw new Error('no path');
    });
    const out = await uploadList(async () => result('a.png'), ['/a/a.png'], h);
    expect(out).toEqual([]);
    expect(h.done).not.toHaveBeenCalled();
    expect(h.fail).toHaveBeenCalledWith('a.png', expect.objectContaining({ message: 'no path' }));
  });

  it('works without the optional hooks', async () => {
    const out = await uploadList(async () => result('a.png'), ['/a/a.png'], { progress: vi.fn(), fail: vi.fn() });
    expect(out).toHaveLength(1);
  });
});

describe('uploadFailures', () => {
  it('has nothing to report when every file went up', async () => {
    const failures = uploadFailures();
    await uploadList(async (p) => result(p), ['/a', '/b'], { progress: vi.fn(), fail: failures.fail });
    expect(failures.summary()).toBeNull();
  });

  it('names the file and the error of a single failure', async () => {
    const failures = uploadFailures();
    await uploadList(async () => Promise.reject(new Error('disk full')), ['/tmp/a.png'], { progress: vi.fn(), fail: failures.fail });
    expect(failures.summary()).toBe("Couldn't upload a.png: disk full");
  });

  it('sums several failures into one notice with the last error', async () => {
    const failures = uploadFailures();
    const upload = vi.fn(async (p: string) => {
      if (p === '/ok') return result('ok');
      throw p === '/x' ? new Error('first') : 'daemon down';
    });
    const done = await uploadList(upload, ['/x', '/ok', '/y'], { progress: vi.fn(), fail: failures.fail });
    expect(done).toHaveLength(1);
    expect(failures.summary()).toBe("Couldn't upload 2 files, last error: daemon down");
  });
});
