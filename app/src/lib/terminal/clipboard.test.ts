import { describe, expect, it } from 'vitest';
import { readClip } from './clipboard';

const png = new File([new Uint8Array([1])], 'paste.png', { type: 'image/png' });

describe('readClip', () => {
  it('returns text when the clipboard has text', async () => {
    expect(await readClip({ text: async () => 'hi', image: async () => null })).toEqual({ kind: 'text', text: 'hi' });
  });

  it('prefers the text when it holds both text and an image', async () => {
    expect(await readClip({ text: async () => 'cells', image: async () => png })).toEqual({ kind: 'text', text: 'cells' });
  });

  it('returns the image when there is no text', async () => {
    expect(await readClip({ text: async () => '', image: async () => png })).toEqual({ kind: 'image', file: png });
  });

  it('returns the image when reading text rejects, as the native clipboard does for an image', async () => {
    const text = async () => Promise.reject(new Error('content not available'));
    expect(await readClip({ text, image: async () => png })).toEqual({ kind: 'image', file: png });
  });

  it('returns nothing when the clipboard is empty', async () => {
    expect(await readClip({ text: async () => '', image: async () => null })).toEqual({ kind: 'none' });
  });

  it('returns nothing when both reads reject', async () => {
    const fail = async () => Promise.reject(new Error('denied'));
    expect(await readClip({ text: fail, image: fail })).toEqual({ kind: 'none' });
  });
});
