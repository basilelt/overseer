import { ICONS, type IconName } from './paths';

export const SPRITE_ID = 'sb-icon-sprite';

export const iconHref = (name: IconName) => `#sbi-${name}`;

export function spriteMarkup(): string {
  return Object.entries(ICONS)
    .map(([name, body]) => `<symbol id="sbi-${name}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</symbol>`)
    .join('');
}

export function ensureSprite(doc: Document | undefined = globalThis.document): boolean {
  if (!doc) return false;
  if (!doc.body) {
    doc.addEventListener('DOMContentLoaded', () => ensureSprite(doc), { once: true });
    return false;
  }
  if (doc.getElementById(SPRITE_ID)) return true;
  const svg = doc.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.id = SPRITE_ID;
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('width', '0');
  svg.setAttribute('height', '0');
  svg.setAttribute('style', 'position:absolute;width:0;height:0;overflow:hidden');
  svg.innerHTML = spriteMarkup();
  doc.body.prepend(svg);
  return true;
}
