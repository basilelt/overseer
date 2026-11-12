export interface CssRule {
  at: string;
  selector: string;
  decls: Record<string, string>;
}

function splitTop(s: string, sep: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = '';
  for (const c of s) {
    if (c === '(' || c === '[') depth++;
    if (c === ')' || c === ']') depth--;
    if (c === sep && depth === 0) {
      out.push(cur);
      cur = '';
    } else cur += c;
  }
  out.push(cur);
  return out;
}

function walk(css: string, at: string, into: CssRule[]): void {
  let i = 0;
  while (i < css.length) {
    const open = css.indexOf('{', i);
    if (open < 0) return;
    const header = css.slice(i, open).trim();
    let depth = 1;
    let j = open + 1;
    for (; j < css.length && depth > 0; j++) {
      if (css[j] === '{') depth++;
      else if (css[j] === '}') depth--;
    }
    const body = css.slice(open + 1, j - 1);
    i = j;
    if (header.startsWith('@')) {
      walk(body, at ? `${at} ${header}` : header, into);
      continue;
    }
    const decls: Record<string, string> = {};
    for (const part of splitTop(body, ';')) {
      const colon = part.indexOf(':');
      if (colon > 0) decls[part.slice(0, colon).trim()] = part.slice(colon + 1).trim();
    }
    for (const selector of splitTop(header, ',')) into.push({ at, selector: selector.trim().replace(/\s+/g, ' '), decls });
  }
}

export function parseStyle(source: string): CssRule[] {
  const start = source.indexOf('<style');
  const css = (start < 0 ? source : source.slice(source.indexOf('>', start) + 1, source.lastIndexOf('</style>'))).replace(/\/\*[\s\S]*?\*\//g, '');
  const rules: CssRule[] = [];
  walk(css, '', rules);
  return rules;
}

export function declOf(source: string, selector: string, prop: string, at = ''): string | undefined {
  let value: string | undefined;
  for (const r of parseStyle(source)) if (r.selector === selector && r.at === at && prop in r.decls) value = r.decls[prop];
  return value;
}
