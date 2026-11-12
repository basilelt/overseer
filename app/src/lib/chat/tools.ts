import type { ChatTool } from '../types';

export interface MiniLine {
  kind: 'add' | 'del' | 'ctx';
  text: string;
}

export interface MiniDiff {
  lines: MiniLine[];
  added: number;
  removed: number;
  more: number;
}

const LCS_MAX = 300;

function lineDiff(a: string[], b: string[]): MiniLine[] {
  if (a.length > LCS_MAX || b.length > LCS_MAX) return [...a.map((text) => ({ kind: 'del' as const, text })), ...b.map((text) => ({ kind: 'add' as const, text }))];
  const n = a.length;
  const m = b.length;
  const dp = Array.from({ length: n + 1 }, () => new Uint16Array(m + 1));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const out: MiniLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ kind: 'ctx', text: a[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) out.push({ kind: 'del', text: a[i++] });
    else out.push({ kind: 'add', text: b[j++] });
  }
  while (i < n) out.push({ kind: 'del', text: a[i++] });
  while (j < m) out.push({ kind: 'add', text: b[j++] });
  return out;
}

function patchLines(patch: string): MiniLine[] {
  const out: MiniLine[] = [];
  for (const l of patch.split('\n')) {
    if (/^(\*\*\*|@@|\+\+\+ |--- |diff |index )/.test(l)) continue;
    if (l.startsWith('+')) out.push({ kind: 'add', text: l.slice(1) });
    else if (l.startsWith('-')) out.push({ kind: 'del', text: l.slice(1) });
    else if (l.startsWith(' ')) out.push({ kind: 'ctx', text: l.slice(1) });
  }
  return out;
}

function trimContext(lines: MiniLine[], ctx = 2): MiniLine[] {
  const keep = lines.map(() => false);
  lines.forEach((l, i) => {
    if (l.kind === 'ctx') return;
    for (let k = Math.max(0, i - ctx); k <= Math.min(lines.length - 1, i + ctx); k++) keep[k] = true;
  });
  return lines.filter((_, i) => keep[i]);
}

export function miniDiff(diff: ChatTool['diff'], max = 40): MiniDiff | null {
  if (!diff) return null;
  const split = (s: string) => (s ? s.replace(/\n$/, '').split('\n') : []);
  const all = 'patch' in diff ? patchLines(diff.patch) : lineDiff(split(diff.old), split(diff.new));
  const lines = trimContext(all);
  return {
    lines: lines.slice(0, max),
    added: all.filter((l) => l.kind === 'add').length,
    removed: all.filter((l) => l.kind === 'del').length,
    more: Math.max(0, lines.length - max),
  };
}

export function clip(text: string, maxLines = 12): { text: string; more: number } {
  const lines = text.replace(/\s+$/, '').split('\n');
  if (lines.length <= maxLines) return { text: lines.join('\n'), more: 0 };
  return { text: lines.slice(0, maxLines).join('\n'), more: lines.length - maxLines };
}

export function toolSummary(t: ChatTool): string {
  if (t.kind === 'bash' && t.command && (!t.label || t.label === t.name)) return t.command.split('\n')[0];
  return t.label || t.name;
}

export function hasDetail(t: ChatTool): boolean {
  return !!(t.diff || t.output || t.command || t.detail || t.path);
}

