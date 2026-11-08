interface CopyHost {
  copyToClipboard?: (text: string) => void;
}

type ListenerHost = Pick<EventTarget, 'addEventListener' | 'removeEventListener'>;

// ghostty-web writes the selection (drag, double/triple click, Cmd+C) through navigator.clipboard with an
// execCommand fallback; the macOS webview rejects both, so the copy is lost. Route every copy through the app.
export function routeCopies(sm: CopyHost | undefined, copy: (text: string) => Promise<void>) {
  if (!sm?.copyToClipboard) return;
  sm.copyToClipboard = (text) => {
    if (text) copy(text).catch(() => {});
  };
}

// ghostty-web ends mouse selection on document mouseup. Native drags and focus changes can cancel
// the pointer before WebKit delivers that event, leaving its selection manager stuck in drag mode.
export function finishSelectionOnInterruption(win: ListenerHost, terminal: ListenerHost, doc: Pick<EventTarget, 'dispatchEvent'>): () => void {
  const finish = () => doc.dispatchEvent(new Event('mouseup', { bubbles: true }));
  win.addEventListener('blur', finish);
  terminal.addEventListener('pointercancel', finish);
  return () => {
    win.removeEventListener('blur', finish);
    terminal.removeEventListener('pointercancel', finish);
  };
}
