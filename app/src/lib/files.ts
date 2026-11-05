import type { DaemonClient } from './client';
import { api } from './api';

export type FileKind = 'image' | 'video' | 'audio' | 'pdf' | 'markdown' | 'text';

const KINDS: Record<string, FileKind> = {
  png: 'image', jpg: 'image', jpeg: 'image', gif: 'image', webp: 'image', svg: 'image', bmp: 'image', ico: 'image', avif: 'image',
  mp4: 'video', webm: 'video', mov: 'video', m4v: 'video', ogv: 'video',
  mp3: 'audio', wav: 'audio', ogg: 'audio', m4a: 'audio', flac: 'audio', opus: 'audio',
  pdf: 'pdf',
  md: 'markdown', markdown: 'markdown',
  txt: 'text', log: 'text', csv: 'text', tsv: 'text', json: 'text', jsonl: 'text', yaml: 'text', yml: 'text', toml: 'text', xml: 'text', html: 'text',
};

const MIME: Record<string, string> = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml', bmp: 'image/bmp', ico: 'image/x-icon', avif: 'image/avif',
  mp4: 'video/mp4', webm: 'video/webm', mov: 'video/quicktime', m4v: 'video/mp4', ogv: 'video/ogg',
  mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg', m4a: 'audio/mp4', flac: 'audio/flac', opus: 'audio/ogg',
  pdf: 'application/pdf',
};

function ext(path: string): string {
  const name = path.slice(path.lastIndexOf('/') + 1);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

export function fileKind(path: string): FileKind | null {
  return KINDS[ext(path)] ?? null;
}

export function mimeOf(path: string): string {
  return MIME[ext(path)] ?? 'text/plain';
}

const EXTS = Object.keys(KINDS).join('|');
// Segments end with '/', which they cannot contain, so each split is unique: no nested-quantifier backtracking.
const PATH_RE = new RegExp(`(?:~|\\.{1,2})?/?(?:[\\w.@+-]+/)*[\\w.@+-]+\\.(?:${EXTS})(?::\\d+){0,2}(?![\\w/])`, 'gi');
// Paths contain no whitespace; longer tokens (base64, minified code) are skipped to bound the scan.
const TOKEN_RE = /\S+/g;
const MAX_TOKEN = 1024;

export interface PathMatch {
  path: string;
  start: number;
  end: number;
}

export function findPaths(text: string): PathMatch[] {
  const out: PathMatch[] = [];
  for (const t of text.matchAll(TOKEN_RE)) {
    const token = t[0];
    if (token.length > MAX_TOKEN || !token.includes('.')) continue;
    const base = t.index ?? 0;
    for (const m of token.matchAll(PATH_RE)) {
      const start = base + (m.index ?? 0);
      if (start > 0 && /[\w:/]/.test(text[start - 1])) continue;
      out.push({ path: m[0].replace(/(?::\d+){1,2}$/, ''), start, end: start + m[0].length });
    }
  }
  return out;
}

const SPACED_SEG = '[\\w.@+()-]+(?: [\\w.@+()-]+)*';
const SPACED_RE = new RegExp(`^(?:~|\\.{1,2})?/?(?:${SPACED_SEG}/)*${SPACED_SEG}\\.(?:${EXTS})(?::\\d+){0,2}$`, 'i');
const MAX_SPACED = 512;
const PROSE_LEAD = /^(?:see|run|open|opens|check|edit|read|view|in|at|on|to|from|with|using|use|voir|ouvrir|ouvre|lancer|lance|dans|sur|avec|lire|éditer)\s/i;

export function filePath(text: string): string | null {
  const t = text.trim();
  const m = findPaths(t);
  if (m.length === 1 && m[0].start === 0 && m[0].end === t.length) return m[0].path;
  if (!t.includes(' ') || t.length > MAX_SPACED || !SPACED_RE.test(t)) return null;
  if (PROSE_LEAD.test(t) && SPACED_RE.test(t.replace(PROSE_LEAD, ''))) return null;
  return t.replace(/(?::\d+){1,2}$/, '');
}

export async function saveFile(client: DaemonClient, file: { path: string; term?: string; line?: string }, name: string): Promise<string> {
  const { path, term, line } = file;
  const buf = await api(client).fileBytes({ path, ...(term && { term }), ...(line && { line }) });
  return client.saveDownload(name, new Uint8Array(buf));
}
