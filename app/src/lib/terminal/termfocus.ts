type Focusable = { focus(options?: FocusOptions): void };

// ghostty-web focuses its contenteditable container on open() and Terminal.focus(), while a tap on
// the canvas focuses its textarea. Keep soft keyboards on the textarea, the input a tap uses, also
// when the terminal mounts with the keyboard already open (a new session from the phone palette).
export function keepInputFocus(container: EventTarget, input: Focusable | null, doc: { activeElement: unknown } | undefined = globalThis.document): () => void {
  if (!input) return () => {};
  const onFocus = (e: Event) => {
    if (e.target === container) input.focus({ preventScroll: true });
  };
  container.addEventListener('focus', onFocus);
  if (doc?.activeElement === container) input.focus({ preventScroll: true });
  return () => container.removeEventListener('focus', onFocus);
}
