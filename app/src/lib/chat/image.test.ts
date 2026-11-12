import { describe, expect, it, vi } from 'vitest';
import { api } from '../daemon/api';
import type { DaemonClient } from '../daemon/client';
import { chatImage } from './image';
import { quietMock } from '../../test/quiet-mock';

const mock = () => quietMock();

describe('chatImage', () => {
  it('builds the route with the encoded line id, offset and index', async () => {
    const fetchBytes = vi.fn(async () => new ArrayBuffer(0));
    await chatImage({ fetchBytes } as unknown as DaemonClient, 'ln/1 a', 120, 2);
    expect(fetchBytes.mock.calls).toEqual([['/v1/lines/ln%2F1%20a/chat/image?at=120&i=2']]);
  });

  it('serves the mock PNG for each image of an entry the chat page lists', async () => {
    const m = mock();
    const u = (await api(m).chat('ln_01')).entries.find((e) => e.images);
    expect(u).toMatchObject({ images: 2, imagesAt: 0 });
    for (const i of [0, 1]) expect([...new Uint8Array(await chatImage(m, 'ln_01', u!.imagesAt!, i)).slice(1, 4)]).toEqual([0x50, 0x4e, 0x47]);
  });

  it('rejects like the daemon: bad query, missing image, unknown line', async () => {
    const m = mock();
    await expect(m.fetchBytes('/v1/lines/ln_01/chat/image?at=x&i=0')).rejects.toMatchObject({ status: 400, message: 'at and i must be non-negative integers' });
    await expect(chatImage(m, 'ln_01', 0, 2)).rejects.toMatchObject({ status: 404, message: 'no image 2 at 0' });
    await expect(chatImage(m, 'ln_01', 7, 0)).rejects.toMatchObject({ status: 404 });
    await expect(chatImage(m, 'nope', 0, 0)).rejects.toMatchObject({ status: 404 });
  });
});
