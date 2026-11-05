import type { ChatEntry } from '../types';

export function sentHistory(entries: ChatEntry[], max = 100): string[] {
  const out: string[] = [];
  for (let i = entries.length - 1; i >= 0 && out.length < max; i--) {
    const e = entries[i];
    const text = e.kind === 'user' ? e.text?.trim() : undefined;
    if (text && !text.startsWith('<') && text !== out[out.length - 1]) out.push(text);
  }
  return out;
}

export interface Recall {
  index: number;
  draft: string;
}

export function recall(history: string[], state: Recall, text: string, dir: 'up' | 'down'): { state: Recall; text: string } | null {
  if (dir === 'up') {
    if (state.index + 1 >= history.length) return null;
    const draft = state.index === -1 ? text : state.draft;
    const index = state.index + 1;
    return { state: { index, draft }, text: history[index] };
  }
  if (state.index === -1) return null;
  const index = state.index - 1;
  return { state: { index, draft: state.draft }, text: index === -1 ? state.draft : history[index] };
}

export function atEdge(text: string, start: number, end: number, dir: 'up' | 'down'): boolean {
  if (start !== end) return false;
  return dir === 'up' ? !text.slice(0, start).includes('\n') : !text.slice(end).includes('\n');
}
