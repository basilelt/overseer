import type { ReviewComment, Thread, ThreadStatus } from './types';

export type Chip = 'pending' | 'queued' | 'delivered' | 'sent' | 'addressed' | 'resolved' | 'changed' | 'outdated';
export type Tone = 'neutral' | 'accent' | 'warn' | 'ok' | 'muted';

export const CHIP_TONE: Record<Chip, Tone> = {
  pending: 'warn',
  queued: 'accent',
  delivered: 'accent',
  sent: 'accent',
  addressed: 'ok',
  resolved: 'muted',
  changed: 'warn',
  outdated: 'muted',
};

export function chips(t: Thread): Chip[] {
  const out: Chip[] = [];
  if (t.status === 'pending') out.push('pending');
  else if (t.status === 'sent') out.push(t.delivery?.status === 'queued' ? 'queued' : t.delivery?.status === 'delivered' ? 'delivered' : 'sent');
  else out.push(t.status);
  if (t.changed && t.status !== 'pending') out.push('changed');
  if (t.outdated) out.push('outdated');
  return out;
}

export const OPEN: ThreadStatus[] = ['pending', 'sent', 'addressed'];
export const isOpen = (t: Thread) => OPEN.includes(t.status);

export function everSent(t: Thread): boolean {
  return !!t.delivery || t.comments.some((c) => !c.pending);
}

export function reopenStatus(t: Thread): ThreadStatus {
  return everSent(t) ? 'sent' : 'pending';
}

export function withStatus(t: Thread, action: 'resolve' | 'reopen'): Thread {
  return { ...t, status: action === 'resolve' ? 'resolved' : reopenStatus(t) };
}

export function pendingComments(threads: Thread[]): ReviewComment[] {
  return threads.flatMap((t) => t.comments.filter((c) => c.pending));
}

export interface Batch {
  comments: number;
  threads: number;
  files: { path: string; count: number }[];
}

export function batch(threads: Thread[]): Batch {
  const files = new Map<string, number>();
  let comments = 0;
  let n = 0;
  for (const t of threads) {
    const c = t.comments.filter((x) => x.pending).length;
    if (!c) continue;
    comments += c;
    n++;
    files.set(t.path, (files.get(t.path) ?? 0) + c);
  }
  return { comments, threads: n, files: [...files].sort((a, b) => a[0].localeCompare(b[0])).map(([path, count]) => ({ path, count })) };
}

export function canSubmit(kind: 'comment' | 'request_changes' | 'approve', b: Batch, body: string): boolean {
  if (kind === 'approve') return true;
  return b.comments > 0 || body.trim().length > 0;
}

export function markSent(threads: Thread[], sent: Thread[]): Thread[] {
  const by = new Map(sent.map((t) => [t.id, t]));
  return threads.map((t) => by.get(t.id) ?? t);
}

export function upsert(threads: Thread[], t: Thread): Thread[] {
  const i = threads.findIndex((x) => x.id === t.id);
  if (i < 0) return [...threads, t];
  const next = threads.slice();
  next[i] = t;
  return next;
}

export function unresolvedByWorktree(threads: Iterable<Pick<Thread, 'worktree' | 'status'>>): Record<string, number> {
  const out: Record<string, number> = {};
  for (const t of threads) if (OPEN.includes(t.status)) out[t.worktree] = (out[t.worktree] ?? 0) + 1;
  return out;
}

export function countByFile(threads: Thread[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const t of threads) if (isOpen(t)) out[t.path] = (out[t.path] ?? 0) + 1;
  return out;
}

const FENCE = /^```([\w-]*)\s*$/;

export type Inline = { code: boolean; text: string };
export type Block = { type: 'p'; inlines: Inline[] } | { type: 'code'; lang: string; text: string } | { type: 'suggestion'; lines: string[] };

export function inlines(text: string): Inline[] {
  const out: Inline[] = [];
  const parts = text.split(/(`[^`\n]+`)/);
  for (const p of parts) {
    if (!p) continue;
    if (p.length > 2 && p.startsWith('`') && p.endsWith('`')) out.push({ code: true, text: p.slice(1, -1) });
    else out.push({ code: false, text: p });
  }
  return out;
}

export function parseBody(body: string): Block[] {
  const blocks: Block[] = [];
  const lines = body.replace(/\r\n/g, '\n').split('\n');
  let para: string[] = [];
  const flush = () => {
    const text = para.join('\n').trim();
    if (text) blocks.push({ type: 'p', inlines: inlines(text) });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(FENCE);
    if (!m) {
      if (!lines[i].trim()) flush();
      else para.push(lines[i]);
      continue;
    }
    flush();
    const inner: string[] = [];
    let j = i + 1;
    while (j < lines.length && !/^```\s*$/.test(lines[j])) inner.push(lines[j++]);
    i = j;
    if (m[1] === 'suggestion') blocks.push({ type: 'suggestion', lines: inner });
    else blocks.push({ type: 'code', lang: m[1], text: inner.join('\n') });
  }
  flush();
  return blocks;
}

export function suggestionOf(body: string): string[] | null {
  const s = parseBody(body).filter((b) => b.type === 'suggestion');
  const last = s.at(-1);
  return last && last.type === 'suggestion' ? last.lines : null;
}

export function threadSuggestion(t: Thread): string[] | null {
  for (let i = t.comments.length - 1; i >= 0; i--) {
    if (t.comments[i].author !== 'user') continue;
    const s = suggestionOf(t.comments[i].body);
    if (s) return s;
  }
  return null;
}

export function suggestionBlock(lines: string[]): string {
  return `\`\`\`suggestion\n${lines.join('\n')}\n\`\`\``;
}

export function insertSuggestion(body: string, lines: string[]): string {
  const block = suggestionBlock(lines);
  const head = body.replace(/\s+$/, '');
  return head ? `${head}\n\n${block}` : block;
}

export interface PreviewLine {
  kind: 'ctx' | 'add' | 'del';
  text: string;
}

export function previewSuggestion(code: string[], suggestion: string[]): PreviewLine[] {
  let pre = 0;
  while (pre < code.length && pre < suggestion.length && code[pre] === suggestion[pre]) pre++;
  let post = 0;
  while (post < code.length - pre && post < suggestion.length - pre && code[code.length - 1 - post] === suggestion[suggestion.length - 1 - post]) post++;
  return [
    ...code.slice(0, pre).map((text) => ({ kind: 'ctx' as const, text })),
    ...code.slice(pre, code.length - post).map((text) => ({ kind: 'del' as const, text })),
    ...suggestion.slice(pre, suggestion.length - post).map((text) => ({ kind: 'add' as const, text })),
    ...code.slice(code.length - post).map((text) => ({ kind: 'ctx' as const, text })),
  ];
}

export function applySuggestion(fileLines: string[], start: number, end: number, suggestion: string[]): string[] {
  return [...fileLines.slice(0, start - 1), ...suggestion, ...fileLines.slice(end)];
}

export function rangeLabel(t: Pick<Thread, 'line' | 'endLine' | 'side'>): string {
  const r = t.line === t.endLine ? `${t.line}` : `${t.line}–${t.endLine}`;
  return t.side === 'old' ? `L${r} (base)` : `L${r}`;
}
