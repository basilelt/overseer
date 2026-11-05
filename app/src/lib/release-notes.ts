export type Inline =
  | { t: 'text'; v: string }
  | { t: 'code'; v: string }
  | { t: 'strong'; v: string }
  | { t: 'link'; v: string; href: string }
  | { t: 'sha'; v: string; href: string };

export type Block = { t: 'h'; level: number; inl: Inline[] } | { t: 'ul'; items: Inline[][] } | { t: 'p'; inl: Inline[] };

const INLINE = /\(\[([0-9a-f]{7,40})\]\(([^\s)]+)\)\)|`([^`]+)`|\*\*([^*]+)\*\*|\[([^\]]+)\]\(([^\s)]+)\)/g;
const SHA = /^[0-9a-f]{7,40}$/;

export function safeUrl(raw: string): string | null {
  try {
    const url = new URL(raw);
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : null;
  } catch {
    return null;
  }
}

export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  const text = (v: string) => {
    if (!v) return;
    const prev = out.at(-1);
    if (prev?.t === 'text') prev.v += v;
    else out.push({ t: 'text', v });
  };
  const sha = (v: string, href: string) => {
    const prev = out.at(-1);
    if (prev?.t === 'text') {
      prev.v = prev.v.trimEnd();
      if (!prev.v) out.pop();
    }
    out.push({ t: 'sha', v: v.slice(0, 7), href });
  };
  let last = 0;
  for (const m of src.matchAll(INLINE)) {
    text(src.slice(last, m.index));
    last = m.index + m[0].length;
    const [all, commit, commitUrl, code, strong, label, href] = m;
    if (commit) {
      const url = safeUrl(commitUrl);
      if (url) sha(commit, url);
      else text(all);
    } else if (code !== undefined) out.push({ t: 'code', v: code });
    else if (strong !== undefined) out.push({ t: 'strong', v: strong });
    else {
      const url = safeUrl(href);
      if (!url || url.includes('/compare/')) text(label);
      else if (SHA.test(label) && url.includes('/commit/')) sha(label, url);
      else out.push({ t: 'link', v: label, href: url });
    }
  }
  text(src.slice(last));
  const first = out[0];
  if (first?.t === 'text') first.v = first.v.trimStart();
  const end = out.at(-1);
  if (end?.t === 'text') end.v = end.v.trimEnd();
  return out.filter((p) => p.t !== 'text' || p.v);
}

export function parseNotes(md: string): Block[] {
  const blocks: Block[] = [];
  for (const line of md.replace(/\r\n?/g, '\n').split('\n')) {
    const heading = /^(#{1,6})\s+(.*)$/.exec(line.trim());
    const bullet = /^\s*[*-]\s+(.*)$/.exec(line);
    if (heading) blocks.push({ t: 'h', level: heading[1].length, inl: parseInline(heading[2]) });
    else if (bullet) {
      const prev = blocks.at(-1);
      const item = parseInline(bullet[1]);
      if (prev?.t === 'ul') prev.items.push(item);
      else blocks.push({ t: 'ul', items: [item] });
    } else if (line.trim()) blocks.push({ t: 'p', inl: parseInline(line) });
    else if (blocks.at(-1)?.t === 'ul') blocks.push({ t: 'p', inl: [] });
  }
  return blocks.filter((b) => b.t !== 'p' || b.inl.length);
}
