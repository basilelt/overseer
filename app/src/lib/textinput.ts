const PLAIN: [string, string][] = [
  ['writingsuggestions', 'false'],
  ['autocorrect', 'off'],
  ['autocapitalize', 'off'],
  ['spellcheck', 'false'],
];
const TEXT_TYPES = new Set(['', 'text', 'search', 'url', 'email', 'password', 'tel', 'number']);

type Field = Pick<Element, 'tagName' | 'hasAttribute' | 'getAttribute' | 'setAttribute'>;

export function isTextField(el: Field): boolean {
  if (el.tagName === 'TEXTAREA') return true;
  return el.tagName === 'INPUT' && TEXT_TYPES.has((el.getAttribute('type') ?? '').toLowerCase());
}

export function applyTextDefaults(el: Field): void {
  if (!isTextField(el)) return;
  const prose = el.getAttribute('spellcheck') === 'true';
  for (const [name, value] of prose ? PLAIN.slice(0, 1) : PLAIN) if (!el.hasAttribute(name)) el.setAttribute(name, value);
}

export function startTextDefaults(doc: Pick<Document, 'addEventListener' | 'removeEventListener'> | undefined = globalThis.document): () => void {
  if (!doc) return () => {};
  const on = (e: Event) => {
    const t = e.target as Field | null;
    if (t && typeof t.tagName === 'string') applyTextDefaults(t);
  };
  doc.addEventListener('focusin', on, true);
  return () => doc.removeEventListener('focusin', on, true);
}
