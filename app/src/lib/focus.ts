export function captureFocus(doc: Pick<Document, 'activeElement' | 'body'> | undefined = typeof document === 'undefined' ? undefined : document): () => void {
  const el = doc?.activeElement;
  return () => {
    if (el && el instanceof HTMLElement && el !== doc!.body && el.isConnected) el.focus();
  };
}

export async function keepFocus<T>(el: HTMLElement | undefined, run: () => Promise<T>, settle: () => Promise<void>): Promise<T> {
  const had = !!el && el.ownerDocument.activeElement === el;
  try {
    return await run();
  } finally {
    if (had && el!.isConnected) {
      await settle();
      el!.focus();
    }
  }
}
