export interface InputLike {
  inputType: string;
  data: string | null;
  isComposing?: boolean;
  replaced?: number;
}

export function imeBytes(e: InputLike): string | null {
  if (e.isComposing) return null;
  switch (e.inputType) {
    case 'insertText':
      return e.data || null;
    case 'insertLineBreak':
    case 'insertParagraph':
      return '\r';
    case 'deleteContentBackward':
      return '\x7f';
    case 'deleteContentForward':
      return '\x1b[3~';
    case 'deleteWordBackward':
      return '\x17';
    case 'insertReplacementText':
      return e.data ? '\x7f'.repeat(e.replaced ?? 0) + e.data : null;
    default:
      return null;
  }
}

interface RangeLike {
  startContainer: unknown;
  endContainer: unknown;
  startOffset: number;
  endOffset: number;
}

interface InputEventLike extends Event {
  inputType?: string;
  data?: string | null;
  isComposing?: boolean;
  dataTransfer?: { getData(type: string): string } | null;
  getTargetRanges?: () => RangeLike[];
}

function replacedLength(e: InputEventLike): number {
  const r = e.getTargetRanges?.()[0];
  return r && r.startContainer === r.endContainer ? Math.max(0, r.endOffset - r.startOffset) : 0;
}

function clear(target: EventTarget | null) {
  if (!target) return;
  const field = target as { value?: unknown };
  if (typeof field.value === 'string') {
    if (field.value !== '') field.value = '';
    return;
  }
  const node = target as { childNodes?: ArrayLike<{ nodeType: number }>; removeChild?: (n: unknown) => void };
  if (!node.childNodes || !node.removeChild) return;
  for (let i = node.childNodes.length - 1; i >= 0; i--) if (node.childNodes[i].nodeType === 3) node.removeChild(node.childNodes[i]);
}

export function attachIme(root: EventTarget, send: (data: string) => void): () => void {
  let lastKey: Event | null = null;
  const keydown = (e: Event) => {
    lastKey = e;
  };
  const keyup = () => {
    lastKey = null;
  };
  const beforeinput = (e: Event) => {
    const ev = e as InputEventLike;
    const handledByKeydown = lastKey?.defaultPrevented ?? false;
    lastKey = null;
    if (handledByKeydown) return;
    const data = ev.data ?? ev.dataTransfer?.getData('text/plain') ?? null;
    const out = imeBytes({ inputType: ev.inputType ?? '', data, isComposing: ev.isComposing, replaced: replacedLength(ev) });
    if (out === null) return;
    e.preventDefault();
    send(out);
  };
  const input = (e: Event) => {
    if (!(e as InputEventLike).isComposing) clear(e.target);
  };
  const compositionend = (e: Event) => clear(e.target);
  const listeners: [string, (e: Event) => void][] = [
    ['keydown', keydown],
    ['keyup', keyup],
    ['beforeinput', beforeinput],
    ['input', input],
    ['compositionend', compositionend],
  ];
  for (const [type, fn] of listeners) root.addEventListener(type, fn);
  return () => {
    for (const [type, fn] of listeners) root.removeEventListener(type, fn);
  };
}
