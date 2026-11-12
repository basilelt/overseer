import type { SidebarItem } from '../session/grouping';

export const TITLE_MAX = 80;

/** Same rules as the daemon: one trimmed line, capped; blank means "back to the auto title". */
export function cleanTitle(raw: string): string | null {
  // eslint-disable-next-line no-control-regex
  const one = raw.replace(/[\u0000-\u001f\u007f-\u009f]/g, ' ').trim();
  return Array.from(one).slice(0, TITLE_MAX).join('').trimEnd() || null;
}

/** What committing an inline title edit means: a new title, null to clear, undefined for no change. */
export function titleEdit(item: Pick<SidebarItem, 'title' | 'renamed'>, draft: string): string | null | undefined {
  const next = cleanTitle(draft);
  if (!item.renamed) return next && next !== item.title ? next : undefined;
  return next === item.title ? undefined : next;
}
