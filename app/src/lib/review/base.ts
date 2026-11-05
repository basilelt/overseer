const BASE_KEY = 'overseer.review.base';

function all(): Record<string, string> {
  try {
    const v = JSON.parse(globalThis.localStorage?.getItem(BASE_KEY) ?? '{}');
    return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
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
  try {
    globalThis.localStorage?.setItem(BASE_KEY, JSON.stringify(m));
  } catch {}
}

export const isSince = (base: string | null | undefined) => !!base && base.endsWith('~1');
