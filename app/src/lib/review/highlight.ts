import { Lru } from '../lru';

const EXT: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  go: 'go',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  css: 'css',
  scss: 'css',
  html: 'xml',
  htm: 'xml',
  xml: 'xml',
  svg: 'xml',
  svelte: 'xml',
  vue: 'xml',
  md: 'markdown',
  toml: 'ini',
  ini: 'ini',
  cfg: 'ini',
  sql: 'sql',
  java: 'java',
  c: 'c',
  h: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  hpp: 'cpp',
  diff: 'diff',
  patch: 'diff',
};

export function langFor(path: string): string | null {
  const name = path.split('/').pop() ?? '';
  if (/^dockerfile/i.test(name)) return 'dockerfile';
  if (/^(gnu)?makefile$/i.test(name) || name.endsWith('.mk')) return 'makefile';
  const ext = name.includes('.') ? name.split('.').pop()!.toLowerCase() : '';
  return EXT[ext] ?? null;
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}

const TAG = /<span class="([\w\s-]*)">|<\/span>/g;

export function splitHtml(html: string): string[] {
  const out: string[] = [];
  const open: string[] = [];
  let cur = '';
  let last = 0;
  const text = (s: string) => {
    s.split('\n').forEach((p, i) => {
      if (i > 0) {
        cur += '</span>'.repeat(open.length);
        out.push(cur);
        cur = open.map((c) => `<span class="${c}">`).join('');
      }
      cur += p;
    });
  };
  for (const m of html.matchAll(TAG)) {
    text(html.slice(last, m.index));
    if (m[1] !== undefined) {
      open.push(m[1]);
      cur += m[0];
    } else {
      open.pop();
      cur += '</span>';
    }
    last = m.index! + m[0].length;
  }
  text(html.slice(last));
  out.push(cur);
  return out;
}

type Renderer = (code: string, language: string) => string;
let loader: Promise<Renderer> | null = null;

function load(): Promise<Renderer> {
  loader ??= import('./hljs').then((m) => m.render);
  return loader;
}

export interface Highlighted {
  new: Map<number, string>;
  old: Map<number, string>;
}

export async function highlightHunks(path: string, hunks: { lines: { kind: string; old?: number; new?: number; text: string }[] }[], extra?: { start: number; lines: string[] }): Promise<Highlighted | null> {
  if (!langFor(path)) return null;
  const out: Highlighted = { new: new Map(), old: new Map() };
  for (const side of ['new', 'old'] as const) {
    const nos: number[] = [];
    const texts: string[] = [];
    for (const h of hunks) {
      for (const l of h.lines) {
        const no = side === 'new' ? (l.kind !== 'del' ? l.new : undefined) : l.kind !== 'add' ? l.old : undefined;
        if (no === undefined) continue;
        nos.push(no);
        texts.push(l.text);
      }
    }
    const html = await highlightLines(texts, path);
    if (html) nos.forEach((n, i) => out[side].set(n, html[i]));
  }
  if (extra) {
    const html = await highlightLines(extra.lines, path);
    if (html) html.forEach((h, i) => out.new.has(extra.start + i) || out.new.set(extra.start + i, h));
  }
  return out;
}

export async function highlightLines(lines: string[], path: string): Promise<string[] | null> {
  const lang = langFor(path);
  if (!lang || !lines.length) return null;
  try {
    const render = await load();
    const out = splitHtml(render(lines.join('\n'), lang));
    return out.length === lines.length ? out : null;
  } catch {
    return null;
  }
}

const LANG_NAMES = new Set([...Object.values(EXT), 'dockerfile', 'makefile']);
const LANG_ALIAS: Record<string, string> = { shell: 'bash', console: 'bash', shellscript: 'bash', golang: 'go', py3: 'python', rs: 'rust', html: 'xml', toml: 'ini' };

export function langOf(tag: string): string | null {
  const t = tag.toLowerCase();
  if (LANG_NAMES.has(t)) return t;
  return LANG_ALIAS[t] ?? EXT[t] ?? null;
}

// Keyed by the code string itself (shared with the parsed markdown), so remounts skip hljs.
const highlighted = new Lru<string, { lang: string; html: string }>(200);

export function cachedHighlight(code: string, tag: string): string | null {
  const lang = langOf(tag);
  const hit = lang ? highlighted.get(code) : undefined;
  return hit && hit.lang === lang ? hit.html : null;
}

export async function highlightCode(code: string, tag: string): Promise<string | null> {
  const lang = langOf(tag);
  if (!lang || !code) return null;
  const hit = cachedHighlight(code, tag);
  if (hit !== null) return hit;
  try {
    const html = (await load())(code, lang);
    highlighted.set(code, { lang, html });
    return html;
  } catch {
    return null;
  }
}
