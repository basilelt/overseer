export type Strings = Record<string, string>;

export function missing(en: Strings, dict: Strings): string[] {
  return Object.keys(en).filter((k) => typeof dict[k] !== 'string');
}

const holes = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',');

function quote(s: string): string {
  return s.includes("'") ? JSON.stringify(s) : `'${JSON.stringify(s).slice(1, -1).replace(/\\"/g, '"')}'`;
}

export function render(en: Strings, dict: Strings): string {
  const rows = Object.keys(en)
    .filter((k) => typeof dict[k] === 'string')
    .map((k) => `  ${quote(k)}: ${quote(dict[k])},\n`)
    .join('');
  return `import type { Dict } from '../i18n.svelte';\n\nexport const dict: Partial<Dict> = {\n${rows}};\n`;
}

export function prompt(locale: string, en: Strings, keys: string[]): string {
  const src = Object.fromEntries(keys.map((k) => [k, en[k]]));
  return [
    `Translate the UI strings of Overseer, a desktop app to run and supervise coding agents, from English to the language with BCP 47 tag "${locale}".`,
    'Keep every {placeholder} exactly as written, keep product, command and file names (Overseer, Claude Code, GitHub, gh, brew, SSH, PR) untranslated, match the tone of a concise native app and keep symbols like …, →, ⌘ and quotes.',
    'Reply with only a JSON object mapping each key to its translation, no prose and no code fence.',
    JSON.stringify(src, null, 1),
  ].join('\n\n');
}

export function parseReply(reply: string, en: Strings, keys: string[]): Strings {
  const start = reply.indexOf('{');
  const end = reply.lastIndexOf('}');
  if (start < 0 || end < start) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(reply.slice(start, end + 1));
  } catch {
    return {};
  }
  if (!parsed || typeof parsed !== 'object') return {};
  const got = parsed as Record<string, unknown>;
  const out: Strings = {};
  for (const k of keys) {
    const v = got[k];
    if (typeof v === 'string' && v.trim() && holes(v) === holes(en[k])) out[k] = v;
  }
  return out;
}
