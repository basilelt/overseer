import { readJson, writeJson, KEY_REVIEW_BASE } from '../storage';

const BASE_KEY = KEY_REVIEW_BASE;

function all(): Record<string, string> {
  const v = readJson(BASE_KEY);
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, string>) : {};
}

export function loadBase(worktree: string | null | undefined): string | null {
  if (!worktree) return null;
  const v = all()[worktree];
  return typeof v === 'string' && v ? v : null;
}

export function saveBase(worktree: string | null | undefined, base: string | null) {
  if (!worktree) return;
  const m = all();
  if (base) m[worktree] = base;
  else delete m[worktree];
  writeJson(BASE_KEY, m);
}

export const isSince = (base: string | null | undefined) => !!base && base.endsWith('~1');
