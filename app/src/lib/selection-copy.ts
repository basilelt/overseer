interface Selectable {
  hasSelection(): boolean;
  getSelection(): string;
  onSelectionChange?: (fn: () => void) => { dispose(): void };
}

// ghostty-web copies a selection itself through navigator.clipboard, which the desktop webview rejects silently.
export function copyOnSelect(t: Selectable, copy: (text: string) => Promise<void>, skip: () => boolean = () => false) {
  return t.onSelectionChange?.(() => {
    if (skip() || !t.hasSelection()) return;
    const text = t.getSelection();
    if (text) copy(text).catch(() => {});
  });
}
