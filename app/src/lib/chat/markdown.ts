import { filePath, findPaths } from '../files';
import { Lru } from '../lru';
import { escapeHtml } from '../review/highlight';

export type Block =
  | { type: 'p'; html: string }
  | { type: 'h'; level: number; html: string }
  | { type: 'list'; ordered: boolean; start: number; items: string[] }
  | { type: 'quote'; html: string }
  | { type: 'code'; lang: string; code: string }
  | { type: 'hr' }
  | { type: 'table'; head: string[]; rows: string[][] };

const SAFE_URL = /^(https?:\/\/|mailto:)[^\s"'<>]+$/i;

function emphasis(s: string): string {
  return s
    .replace(/\*\*(?=\S)((?:[^*]|\*(?!\*))*?\S)\*\*/g, '<strong>$1</strong>')
    .replace(/__(?=\S)((?:[^_]|_(?!_))*?\S)__/g, '<strong>$1</strong>')
    .replace(/(^|[^*\w])\*(?=\S)([^*]*?\S)\*(?!\*)/g, '$1<em>$2</em>')
    .replace(/(^|[^_\w])_(?=\S)([^_]*?\S)_(?![_\w])/g, '$1<em>$2</em>')
    .replace(/~~(?=\S)((?:[^~]|~(?!~))*?\S)~~/g, '<del>$1</del>');
}

function fileLink(path: string, html: string): string {
  return `<a class="file-link" data-file="${escapeHtml(path)}">${html}</a>`;
}

function prose(s: string): string {
  const files: string[] = [];
  let out = '';
  let last = 0;
  for (const m of findPaths(s)) {
    out += `${s.slice(last, m.start)}\u0000${files.length}\u0000`;
    files.push(fileLink(m.path, escapeHtml(s.slice(m.start, m.end))));
    last = m.end;
  }
  return emphasis(escapeHtml(out + s.slice(last))).replace(/\u0000(\d+)\u0000/g, (_, i) => files[+i] ?? '');
}

function links(text: string): string {
  const out: string[] = [];
  let last = 0;
  const re = /\[([^\]\n]+)\]\(([^)\s]+)\)|(https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"])/g;
  for (const m of text.matchAll(re)) {
    out.push(prose(text.slice(last, m.index)));
    const label = m[1] ?? m[3];
    const url = m[2] ?? m[3];
    if (SAFE_URL.test(url)) out.push(`<a href="${escapeHtml(url)}" target="_blank" rel="noopener noreferrer">${emphasis(escapeHtml(label))}</a>`);
    else out.push(emphasis(escapeHtml(m[0])));
    last = m.index! + m[0].length;
  }
  out.push(prose(text.slice(last)));
  return out.join('');
}

export function inline(text: string): string {
  const out: string[] = [];
  let i = 0;
  while (i < text.length) {
    const tick = text.indexOf('`', i);
    if (tick < 0) break;
    let run = 1;
    while (text[tick + run] === '`') run++;
    const fence = '`'.repeat(run);
    const end = text.indexOf(fence, tick + run);
    if (end < 0) break;
    out.push(links(text.slice(i, tick)));
    const code = text.slice(tick + run, end).replace(/^ (.*) $/, '$1');
    const path = filePath(code);
    const html = `<code>${escapeHtml(code)}</code>`;
    out.push(path ? fileLink(path, html) : html);
    i = end + run;
  }
  out.push(links(text.slice(i)));
  return out.join('');
}

const FENCE = /^ {0,3}(`{3,}|~{3,})\s*([\w+#.-]*)/;
const HEADING = /^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const BULLET = /^\s*([-*+]|\d{1,9}[.)])\s+(.*)$/;
const HR = /^ {0,3}([-*_])(\s*\1){2,}\s*$/;
const ROW = /^\s*\|?(.+?)\|?\s*$/;
const SEP = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

function cells(line: string): string[] {
  const m = line.match(ROW);
  return (m ? m[1] : line).split('|').map((c) => inline(c.trim()));
}

export function parseMarkdown(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, '\n').split('\n');
  const blocks: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    if (para.length) blocks.push({ type: 'p', html: para.map(inline).join('<br>') });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = line.match(FENCE);
    if (fence) {
      flush();
      const close = new RegExp(`^ {0,3}${fence[1][0]}{${fence[1].length},}\\s*$`);
      const code: string[] = [];
      for (i++; i < lines.length && !close.test(lines[i]); i++) code.push(lines[i]);
      blocks.push({ type: 'code', lang: fence[2].toLowerCase(), code: code.join('\n') });
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    const h = line.match(HEADING);
    if (h) {
      flush();
      blocks.push({ type: 'h', level: h[1].length, html: inline(h[2]) });
      continue;
    }
    if (HR.test(line)) {
      flush();
      blocks.push({ type: 'hr' });
      continue;
    }
    if (line.includes('|') && i + 1 < lines.length && SEP.test(lines[i + 1])) {
      flush();
      const head = cells(line);
      const rows: string[][] = [];
      for (i += 2; i < lines.length && lines[i].includes('|') && lines[i].trim(); i++) rows.push(cells(lines[i]));
      i--;
      blocks.push({ type: 'table', head, rows });
      continue;
    }
    if (/^\s*>/.test(line)) {
      flush();
      const q: string[] = [];
      for (; i < lines.length && /^\s*>/.test(lines[i]); i++) q.push(lines[i].replace(/^\s*>\s?/, ''));
      i--;
      blocks.push({ type: 'quote', html: q.map(inline).join('<br>') });
      continue;
    }
    const b = line.match(BULLET);
    if (b && !para.length) {
      const ordered = /\d/.test(b[1]);
      const items: string[] = [];
      let cur = b[2];
      for (i++; i < lines.length; i++) {
        const n = lines[i].match(BULLET);
        if (n && /\d/.test(n[1]) === ordered) {
          items.push(cur);
          cur = n[2];
        } else if (n) {
          items.push(cur);
          cur = n[2];
        } else if (lines[i].trim() && /^\s+/.test(lines[i])) cur += `\n${lines[i].trim()}`;
        else break;
      }
      items.push(cur);
      i--;
      blocks.push({ type: 'list', ordered, start: ordered ? parseInt(b[1], 10) : 1, items: items.map((it) => it.split('\n').map(inline).join('<br>')) });
      continue;
    }
    para.push(line.trim());
  }
  flush();
  return blocks;
}

const parsed = new Lru<string, Block[]>(400);

export function parseMarkdownCached(src: string): Block[] {
  let b = parsed.get(src);
  if (!b) parsed.set(src, (b = parseMarkdown(src)));
  return b;
}
