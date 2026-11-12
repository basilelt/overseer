import { clamp } from './clamp';

export const BUNDLED_FONT = 'JetBrains Mono';
export const SYMBOLS_FONT = 'Symbols Nerd Font Mono';
export const FONT_SUGGESTIONS = ['MesloLGS NF', 'JetBrainsMono Nerd Font', 'FiraCode Nerd Font', 'Hack Nerd Font', 'SF Mono', 'Menlo'];
export const SYMBOL_SAMPLE = '\ue0b0\ue0a0\uf113\uf015';

const GENERIC = new Set(['monospace', 'ui-monospace', 'serif', 'sans-serif', 'system-ui', 'cursive', 'fantasy']);

function unquote(name: string): string {
  const n = name.trim();
  const q = n[0];
  return (q === '"' || q === "'") && n.endsWith(q) && n.length >= 2 ? n.slice(1, -1).trim() : n;
}

export function quoteFamily(name: string): string {
  const n = unquote(name);
  if (GENERIC.has(n.toLowerCase())) return n.toLowerCase();
  return `"${n.replace(/["\\]/g, '')}"`;
}

export function fontFamilies(custom: string): string[] {
  const out: string[] = [];
  const add = (name: string) => {
    const n = unquote(name);
    if (!n || GENERIC.has(n.toLowerCase())) return;
    if (!out.some((x) => x.toLowerCase() === n.toLowerCase())) out.push(n);
  };
  for (const part of custom.split(',')) add(part);
  add(BUNDLED_FONT);
  return [...out.filter((x) => x.toLowerCase() !== SYMBOLS_FONT.toLowerCase()), SYMBOLS_FONT];
}

export function fontChain(custom: string): string {
  return [...fontFamilies(custom).map(quoteFamily), 'monospace'].join(', ');
}

export function clampLineHeight(v: number): number {
  return Number.isFinite(v) ? clamp(Math.round(v * 20) / 20, 1, 2) : 1;
}

export function lineMetrics(m: { width: number; height: number; baseline: number }, lineHeight: number) {
  const lh = clampLineHeight(lineHeight);
  const height = Math.ceil(m.height * lh);
  return { ...m, height, baseline: m.baseline + Math.floor((height - m.height) / 2) };
}

export function sameMetrics(a: { width: number; height: number; baseline: number } | undefined, b: { width: number; height: number; baseline: number }): boolean {
  return !!a && a.width === b.width && a.height === b.height && a.baseline === b.baseline;
}
