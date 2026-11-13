export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  isComposing?: boolean;
}

export function keyToInput(e: KeyLike, opts: { shiftEnterNewline: boolean }): string | null {
  if (!opts.shiftEnterNewline || e.isComposing) return null;
  if (e.key === 'Enter' && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) return '\x1b\r';
  return null;
}
