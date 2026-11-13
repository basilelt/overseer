import { afterEach, describe, expect, it, vi } from 'vitest';

const isTauri = vi.hoisted(() => vi.fn(() => false));
vi.mock('../platform', () => ({ isTauri }));

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const image = vi.hoisted(() => ({ size: vi.fn(), rgba: vi.fn(), close: vi.fn() }));
vi.mock('@tauri-apps/api/image', () => ({
  Image: class {
    constructor() {
      Object.assign(this, image);
    }
  },
}));

import { clipboardImage } from './clipboard-image';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  isTauri.mockReturnValue(false);
  invoke.mockReset();
  image.size.mockReset();
  image.rgba.mockReset();
  image.close.mockReset();
});

describe('clipboardImage in a browser', () => {
  it('returns the first image on the clipboard', async () => {
    const blob = new Blob(['x'], { type: 'image/png' });
    vi.stubGlobal('navigator', { clipboard: { read: async () => [{ types: ['text/html'], getType: vi.fn() }, { types: ['image/png'], getType: async () => blob }] } });
    const f = await clipboardImage();
    expect(f?.name).toBe('paste.png');
    expect(f?.type).toBe('image/png');
  });

  it('returns null when the clipboard has no image', async () => {
    vi.stubGlobal('navigator', { clipboard: { read: async () => [{ types: ['text/plain'], getType: vi.fn() }] } });
    expect(await clipboardImage()).toBeNull();
  });

  it('returns null without the async clipboard API', async () => {
    vi.stubGlobal('navigator', {});
    expect(await clipboardImage()).toBeNull();
  });
});

describe('clipboardImage in the desktop app', () => {
  it('encodes the RGBA bytes of the native clipboard image as a PNG and releases the resource', async () => {
    isTauri.mockReturnValue(true);
    invoke.mockResolvedValue(7);
    image.size.mockResolvedValue({ width: 1, height: 1 });
    image.rgba.mockResolvedValue(new Uint8Array([255, 0, 0, 255]));
    image.close.mockResolvedValue(undefined);
    const putImageData = vi.fn();
    const canvas = { width: 0, height: 0, getContext: () => ({ putImageData }), toBlob: (cb: (b: Blob) => void) => cb(new Blob(['png'], { type: 'image/png' })) };
    vi.stubGlobal('document', { createElement: () => canvas });
    vi.stubGlobal('ImageData', class {});
    const f = await clipboardImage();
    expect(invoke).toHaveBeenCalledWith('plugin:clipboard-manager|read_image');
    expect(canvas.width).toBe(1);
    expect(putImageData).toHaveBeenCalledOnce();
    expect(f?.type).toBe('image/png');
    expect(image.close).toHaveBeenCalledOnce();
  });

  it('rejects when the native clipboard holds no image', async () => {
    isTauri.mockReturnValue(true);
    invoke.mockRejectedValue(new Error('content not available'));
    await expect(clipboardImage()).rejects.toThrow('content not available');
  });
});
