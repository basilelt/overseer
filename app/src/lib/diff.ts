export type DiffLineKind = 'add' | 'del' | 'ctx' | 'meta';

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
  oldNo?: number;
  newNo?: number;
}

export interface Hunk {
  header: string;
  lines: DiffLine[];
}

export interface DiffFile {
  oldPath: string;
  newPath: string;
  path: string;
  hunks: Hunk[];
  added: number;
  removed: number;
  binary: boolean;
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

export function parseDiff(text: string): DiffFile[] {
  const files: DiffFile[] = [];
  let file: DiffFile | null = null;
  let hunk: Hunk | null = null;
  let oldNo = 0;
  let newNo = 0;
  for (const raw of text.split('\n')) {
    if (raw.startsWith('diff --git ')) {
      const m = raw.match(/^diff --git a\/(.*) b\/(.*)$/);
      file = { oldPath: m?.[1] ?? '', newPath: m?.[2] ?? '', path: m?.[2] ?? raw.slice(11), hunks: [], added: 0, removed: 0, binary: false };
      files.push(file);
      hunk = null;
      continue;
    }
    if (!file) continue;
    if (!hunk) {
      if (raw.startsWith('--- ')) file.oldPath = stripPrefix(raw.slice(4));
      else if (raw.startsWith('+++ ')) {
        file.newPath = stripPrefix(raw.slice(4));
        file.path = file.newPath === '/dev/null' ? file.oldPath : file.newPath;
      } else if (raw.startsWith('Binary files')) file.binary = true;
    }
    const h = raw.match(HUNK);
    if (h) {
      oldNo = Number(h[1]);
      newNo = Number(h[2]);
      hunk = { header: raw, lines: [] };
      file.hunks.push(hunk);
      continue;
    }
    if (!hunk) continue;
    const c = raw[0];
    if (c === '+') {
      hunk.lines.push({ kind: 'add', text: raw.slice(1), newNo: newNo++ });
      file.added++;
    } else if (c === '-') {
      hunk.lines.push({ kind: 'del', text: raw.slice(1), oldNo: oldNo++ });
      file.removed++;
    } else if (c === '\\') {
      hunk.lines.push({ kind: 'meta', text: raw });
    } else if (c === ' ' || raw === '') {
      hunk.lines.push({ kind: 'ctx', text: raw.slice(1), oldNo: oldNo++, newNo: newNo++ });
    }
  }
  for (const f of files) {
    for (const hk of f.hunks) {
      while (hk.lines.at(-1)?.kind === 'ctx' && hk.lines.at(-1)?.text === '') hk.lines.pop();
    }
  }
  return files;
}

function stripPrefix(p: string): string {
  return p.replace(/^[ab]\//, '').replace(/\t.*$/, '');
}

export type TokenKind = 'kw' | 'str' | 'com' | 'num' | 'txt';
export interface Token {
  kind: TokenKind;
  text: string;
}

const KEYWORDS = new Set(
  'fn let mut pub use impl struct enum trait match if else for while loop return self Self mod const static async await move where type as in dyn ref unsafe crate super function var import export from default class extends new this interface typeof instanceof try catch finally throw switch case break continue def lambda None True False null undefined true false void yield package func go chan defer select'.split(
    ' ',
  ),
);

const TOKEN = /(\/\/.*$|#(?![\[!]).*$|\/\*.*?\*\/)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)|(\b\d[\d_.]*\b)|([A-Za-z_]\w*)/g;

export function tokenize(line: string): Token[] {
  const out: Token[] = [];
  let last = 0;
  for (const m of line.matchAll(TOKEN)) {
    const i = m.index!;
    if (m[4] && !KEYWORDS.has(m[4])) continue;
    if (i > last) out.push({ kind: 'txt', text: line.slice(last, i) });
    out.push({ kind: m[1] ? 'com' : m[2] ? 'str' : m[3] ? 'num' : 'kw', text: m[0] });
    last = i + m[0].length;
  }
  if (last < line.length) out.push({ kind: 'txt', text: line.slice(last) });
  return out;
}
