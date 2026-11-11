import type { Related } from '../review/types';
import type { ScmSubmodule } from './types';

export interface WorktreeChoice {
  path: string;
  name: string;
  branch: string | null;
  files: number;
  submodule: boolean;
  current: boolean;
}

const folder = (p: string) => p.replace(/\/+$/, '').split('/').pop() || p;

export function worktreeChoices(related: Related | null, subs: Record<string, ScmSubmodule[]>, current: string | null | undefined): WorktreeChoice[] {
  const out: WorktreeChoice[] = [];
  for (const w of related?.worktrees ?? []) {
    out.push({ path: w.path, name: folder(w.path), branch: w.branch ?? null, files: w.files, submodule: false, current: w.path === current });
    for (const m of subs[w.path] ?? []) out.push({ path: m.worktree, name: m.path, branch: null, files: 0, submodule: true, current: m.worktree === current });
  }
  if (!out.length && current) out.push({ path: current, name: folder(current), branch: null, files: 0, submodule: false, current: true });
  return out;
}
