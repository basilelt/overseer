export type NativeMenu = 'native' | 'copy' | 'none';

export function isEditable(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === 'TEXTAREA') return true;
  if (tag === 'INPUT') return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file', 'image'].includes((el as HTMLInputElement).type);
  return (el as HTMLElement).isContentEditable === true;
}

export function nativeMenu(shell: { tauri: boolean; phone: boolean }, target: Element | null, selection: string): NativeMenu {
  if (!shell.tauri || shell.phone) return 'native';
  if (isEditable(target)) return 'native';
  return selection.trim() ? 'copy' : 'none';
}
