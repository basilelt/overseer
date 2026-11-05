import type { DaemonClient } from './client';
import { basename, MAX_UPLOAD } from './uploads';

export interface Attachment {
  name: string;
  path: string;
}

type Uploader = Pick<DaemonClient, 'uploadFile' | 'savePaste'>;
type Blob = { name: string; size: number; arrayBuffer(): Promise<ArrayBuffer> };
export type Failure = { name: string; error: string };

const remotePath = (up: { path?: string } | null | undefined) => {
  if (!up?.path) throw new Error('the daemon returned no file path');
  return up.path;
};

const msg = (e: unknown) => String((e as Error)?.message ?? e);

export async function uploadBlobs(client: Uploader, files: Blob[], fail: (f: Failure) => void): Promise<Attachment[]> {
  const out: Attachment[] = [];
  for (const f of files) {
    const name = f.name || 'screenshot.png';
    if (f.size > MAX_UPLOAD) {
      fail({ name, error: '> 50 MB' });
      continue;
    }
    try {
      const local = await client.savePaste(name, new Uint8Array(await f.arrayBuffer()));
      const up = await client.uploadFile(local);
      out.push({ name, path: remotePath(up) });
    } catch (e) {
      fail({ name, error: msg(e) });
    }
  }
  return out;
}

export async function uploadPaths(client: Uploader, paths: string[], fail: (f: Failure) => void): Promise<Attachment[]> {
  const out: Attachment[] = [];
  for (const p of paths) {
    const name = basename(p);
    try {
      const up = await client.uploadFile(p);
      out.push({ name, path: remotePath(up) });
    } catch (e) {
      fail({ name, error: msg(e) });
    }
  }
  return out;
}
