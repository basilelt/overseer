import { basename } from './paths';

export const MAX_UPLOAD = 50 * 1024 * 1024;
export const PROGRESS_FROM = 2 * 1024 * 1024;

export interface FileInfo {
  name: string;
  type: string;
  size: number;
}

export type Refusal = { kind: 'reject'; reason: 'tooLarge' | 'folder'; name: string };

export type PastePlan = { kind: 'native' } | Refusal | { kind: 'save'; upload: boolean; files: FileInfo[] };

export type DropPlan = { kind: 'none' } | Refusal | { kind: 'type'; paths: string[] } | { kind: 'upload'; paths: string[] };

const SAFE = /^[A-Za-z0-9_./:@%+=,-]+$/;

export function shellQuote(path: string): string {
  if (path.startsWith('~/') && SAFE.test(path.slice(2))) return path;
  return SAFE.test(path) ? path : `'${path.replace(/'/g, `'\\''`)}'`;
}

export function insertText(paths: string[]): string {
  return paths.length ? `${paths.map(shellQuote).join(' ')} ` : '';
}

const IMAGE = /\.(png|jpe?g|gif|webp)$/i;

export function uploadedPath(r: { path: string; short?: string }): string {
  return IMAGE.test(r.path) || !r.short ? r.path : r.short;
}

export function bracketed(text: string, on: boolean): string {
  return on && text ? `\x1b[200~${text.replace(/\x1b\[20[01]~/g, '')}\x1b[201~` : text;
}

const isImage = (f: FileInfo) => f.type.startsWith('image/');

export function planPaste(clip: { text: string; files: FileInfo[] }, remote: boolean): PastePlan {
  const { files, text } = clip;
  const textWins = text.trim() !== '' && files.every(isImage) && !files.some((f) => f.name && text.includes(f.name));
  if (!files.length || textWins) return { kind: 'native' };
  const big = files.find((f) => f.size > MAX_UPLOAD);
  if (big) return { kind: 'reject', reason: 'tooLarge', name: big.name };
  return { kind: 'save', upload: remote, files };
}

export function planDrop(drop: { paths: string[]; dirs?: string[] }, remote: boolean): DropPlan {
  if (!drop.paths.length) return { kind: 'none' };
  if (!remote) return { kind: 'type', paths: drop.paths };
  const dir = drop.paths.find((p) => drop.dirs?.includes(p));
  if (dir) return { kind: 'reject', reason: 'folder', name: basename(dir) };
  return { kind: 'upload', paths: drop.paths };
}
