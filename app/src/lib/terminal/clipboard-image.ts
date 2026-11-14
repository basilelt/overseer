import { clipboardReadImage } from '../daemon/tauri-commands';
import { isTauri } from '../platform';

async function toPng(rgba: Uint8Array, width: number, height: number): Promise<File | null> {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, 'image/png'));
  return blob ? new File([blob], 'paste.png', { type: 'image/png' }) : null;
}

async function nativeImage(): Promise<File | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  const { Image } = await import('@tauri-apps/api/image');
  const image = new Image(await invoke<number>(clipboardReadImage));
  try {
    const { width, height } = await image.size();
    return await toPng(await image.rgba(), width, height);
  } finally {
    await image.close().catch(() => {});
  }
}

async function browserImage(): Promise<File | null> {
  for (const item of (await navigator.clipboard?.read?.()) ?? []) {
    const type = item.types.find((x) => x.startsWith('image/'));
    if (type) return new File([await item.getType(type)], `paste.${type.slice(6) || 'png'}`, { type });
  }
  return null;
}

export const clipboardImage = (): Promise<File | null> => (isTauri() ? nativeImage() : browserImage());
