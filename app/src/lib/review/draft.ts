import { insertSuggestion } from './threads';
import type { NewThread, ReviewTarget, Side } from './types';

export interface Draft {
  path: string;
  side: Side;
  anchor: number;
  start: number;
  end: number;
  body: string;
  selecting: boolean;
}

export function begin(path: string, side: Side, line: number): Draft {
  return { path, side, anchor: line, start: line, end: line, body: '', selecting: true };
}

export function extend(d: Draft | null, path: string, side: Side, line: number): Draft | null {
  if (!d || d.path !== path || d.side !== side) return d;
  return { ...d, start: Math.min(d.anchor, line), end: Math.max(d.anchor, line) };
}

export function click(d: Draft | null, path: string, side: Side, line: number, shift: boolean): Draft {
  if (shift && d && d.path === path && d.side === side) return { ...extend(d, path, side, line)!, selecting: false };
  if (d && d.body.trim() && d.path === path && d.side === side && d.start === line && d.end === line) return { ...d, selecting: false };
  return { ...begin(path, side, line), selecting: true };
}

export function finish(d: Draft | null): Draft | null {
  return d ? { ...d, selecting: false } : d;
}

export function setBody(d: Draft, body: string): Draft {
  return { ...d, body };
}

export function suggest(d: Draft, lines: string[]): Draft {
  return { ...d, body: insertSuggestion(d.body, lines) };
}

export function ready(d: Draft | null): boolean {
  return !!d && d.body.trim().length > 0;
}

export type KeyAction = 'submit' | 'cancel' | null;

export function keyAction(e: { key: string; ctrlKey?: boolean; metaKey?: boolean }): KeyAction {
  if (e.key === 'Escape') return 'cancel';
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) return 'submit';
  return null;
}

export function toNewThread(d: Draft, target: ReviewTarget): NewThread {
  return { ...target, path: d.path, side: d.side, line: d.start, endLine: d.end, body: d.body.trim() };
}

export function covers(d: Draft | null, path: string, side: Side, line: number): boolean {
  return !!d && d.path === path && d.side === side && line >= d.start && line <= d.end;
}
