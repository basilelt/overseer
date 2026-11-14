import { TEXT_FIELD, holdFocus, walkDirection } from './walk-key';

type KeyLike = Parameters<typeof walkDirection>[0];
type TargetLike = NonNullable<Parameters<typeof walkDirection>[1]>;

export type ReviewAction = 'next' | 'prev' | 'clear' | 'open' | 'toggleView' | 'allLinks' | Arrow;
export type Arrow = 'left' | 'right' | 'up' | 'down';

const ARROWS: Record<string, Arrow> = { ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down' };
const CONTROL = 'button, a[href], summary, input, [role=radio]';
export const PICKABLE = '[data-pick]';
export const RANK = '[data-rank]';

export function reviewAction(e: KeyLike, target: TargetLike | null, view: 'list' | 'map'): ReviewAction | null {
  if (e.ctrlKey || e.metaKey || e.altKey || !target || target.closest(TEXT_FIELD)) return null;
  const walk = walkDirection(e, target);
  if (walk) return walk > 0 ? 'next' : 'prev';
  if (e.key === 'm') return 'toggleView';
  if (view !== 'map') return null;
  if (e.key === 'a') return 'allLinks';
  if (e.key === 'Escape') return 'clear';
  if (e.key === 'Enter') return !target.closest(CONTROL) || target.closest(PICKABLE) ? 'open' : null;
  const arrow = ARROWS[e.key];
  if (!arrow || target.closest('[role=radiogroup], details')) return null;
  if (target.closest(RANK)) return arrow === 'up' ? 'prev' : arrow === 'down' ? 'next' : null;
  return arrow;
}

export function keepFocus(root: HTMLElement | null | undefined): boolean {
  if (root?.contains(document.activeElement)) return true;
  return holdFocus(root);
}
