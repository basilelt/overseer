interface CopyHost {
  copyToClipboard?: (text: string) => void;
}

// ghostty-web writes the selection (drag, double/triple click, Cmd+C) through navigator.clipboard with an
// execCommand fallback; the macOS webview rejects both, so the copy is lost. Route every copy through the app.
export function routeCopies(sm: CopyHost | undefined, copy: (text: string) => Promise<void>) {
  if (!sm?.copyToClipboard) return;
  sm.copyToClipboard = (text) => {
    if (text) copy(text).catch(() => {});
  };
}
