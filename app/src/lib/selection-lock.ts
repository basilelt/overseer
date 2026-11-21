export function lockSelection(): () => void {
  if (typeof document === 'undefined') return () => {};
  const stop = (e: Event) => e.preventDefault();
  document.getSelection()?.removeAllRanges();
  document.addEventListener('selectstart', stop);
  return () => document.removeEventListener('selectstart', stop);
}
