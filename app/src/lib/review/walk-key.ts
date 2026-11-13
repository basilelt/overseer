export const TEXT_FIELD = 'input:not([type=checkbox]):not([type=radio]):not([type=button]), textarea, select, [contenteditable="true"]';

interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

interface TargetLike {
  closest(selector: string): unknown;
}

export function walkDirection(e: KeyLike, target: TargetLike | null): 1 | -1 | null {
  if (e.ctrlKey || e.metaKey || e.altKey || (e.key !== 'j' && e.key !== 'k')) return null;
  if (!target || target.closest(TEXT_FIELD)) return null;
  return e.key === 'j' ? 1 : -1;
}

export function holdFocus(root: HTMLElement | null | undefined): boolean {
  if (!root?.isConnected) return false;
  if (document.activeElement !== root) root.focus({ preventScroll: true });
  return document.activeElement === root;
}
