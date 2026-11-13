export const PASTE_TARGETS = 'input, textarea, [contenteditable]:not([contenteditable="false"]), [data-terminal]';

export function pastesOnMiddleClick(target: EventTarget | null): boolean {
  const el = target as Element | null;
  return typeof el?.closest === 'function' && el.closest(PASTE_TARGETS) !== null;
}

export function blockStrayMiddlePaste(e: MouseEvent) {
  if (e.button === 1 && !pastesOnMiddleClick(e.target)) e.preventDefault();
}
