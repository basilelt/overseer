export type Clip = { kind: 'text'; text: string } | { kind: 'image'; file: File } | { kind: 'none' };

export interface ClipSource {
  text: () => Promise<string>;
  image: () => Promise<File | null>;
}

export async function readClip(src: ClipSource): Promise<Clip> {
  const text = await src.text().catch(() => '');
  if (text) return { kind: 'text', text };
  const file = await src.image().catch(() => null);
  return file ? { kind: 'image', file } : { kind: 'none' };
}
