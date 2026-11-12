import { basename } from './paths';
import type { UploadProgress, UploadResult } from './types';
import { PROGRESS_FROM } from './uploads';

export async function savePasteAll(files: File[], save: (name: string, bytes: Uint8Array) => Promise<string>, fail: (name: string, e: unknown) => void): Promise<string[] | null> {
  const saved: string[] = [];
  for (const f of files) {
    const name = f.name || 'paste.png';
    try {
      saved.push(await save(name, new Uint8Array(await f.arrayBuffer())));
    } catch (e) {
      fail(name, e);
      return null;
    }
  }
  return saved;
}

export interface UploadHooks {
  progress(name: string, fraction: number): void;
  validate?(r: UploadResult | undefined): void;
  done?(name: string, r: UploadResult, large: boolean): void;
  fail(name: string, e: unknown): void;
}

export async function uploadList(upload: (path: string, onProgress: (p: UploadProgress) => void) => Promise<UploadResult>, paths: string[], hooks: UploadHooks): Promise<UploadResult[]> {
  const done: UploadResult[] = [];
  for (const p of paths) {
    const name = basename(p);
    let large = false;
    try {
      const r = await upload(p, ({ sent, total }) => {
        if (total <= PROGRESS_FROM) return;
        large = true;
        hooks.progress(name, sent / total);
      });
      hooks.validate?.(r);
      done.push(r);
      hooks.done?.(name, r, large);
    } catch (e) {
      hooks.fail(name, e);
    }
  }
  return done;
}
