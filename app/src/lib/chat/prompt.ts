export interface PromptOption {
  label: string;
  keys: string[];
  description?: string;
  input?: boolean;
  chat?: boolean;
  checked?: boolean;
}

export interface PromptTab {
  label: string;
  done: boolean;
  current: boolean;
}

export interface PromptCard {
  question: string;
  options: PromptOption[];
  tabs?: PromptTab[];
  submit?: PromptOption;
  cancel?: boolean;
}

const BOX = /^\s*[│┃|╭╮╰╯▌]|[\s│┃|╭╮╰╯▌]+$/g;
const MARK = /^[❯›>▶●◉]\s*/;
const NUMBERED = /^(?:[❯›>▶●◉]\s*)?(\d{1,2})[.)]\s+(.+)$/;
const SHORTCUT = /\s*\((y|n|a|p|esc|enter)\)\s*$/i;
const RULE = /^[─━═╌\-\s]+$/;
const TABS = /^\s*←.*→\s*$/;
const TAB = /([☐□◻☒■◼✔✓])\s+(.+?)(?=\s+[☐□◻☒■◼✔✓]\s|\s*→|$)/g;
const CHECK = /^\[([ ✔✓x×])\]\s*/;
const SUBMIT = /^(?:[❯›>▶]\s*)?(Submit|Next)$/;
const TYPE = /^Type something\.?$/;

function clean(l: string): string {
  return l.replace(BOX, '');
}

function shortcut(label: string): string | null {
  const m = label.match(SHORTCUT);
  return m ? m[1].toLowerCase() : null;
}

function label(text: string): string {
  return text.replace(SHORTCUT, '').trim();
}

function questionAbove(lines: string[], first: number): string {
  const out: string[] = [];
  let i = first - 1;
  while (i >= 0 && !lines[i].trim()) i--;
  for (; i >= 0 && out.length < 6; i--) {
    const t = lines[i].trim();
    if (!t || RULE.test(t) || TABS.test(t)) break;
    out.unshift(t);
  }
  return out.join('\n');
}

function moves(from: number, to: number): string[] {
  return Array(Math.abs(to - from)).fill(to > from ? 'down' : 'up');
}

function arrows(from: number, to: number): string[] {
  return [...moves(from, to), 'enter'];
}

function above(lines: string[], first: number, re: RegExp): number {
  for (let i = first - 1; i >= 0; i--) if (re.test(lines[i])) return i;
  return -1;
}

function tabs(lines: string[], first: number, review: boolean): PromptTab[] | undefined {
  const at = above(lines, first, TABS);
  if (at < 0) return undefined;
  const line = lines[at];
  const out = [...line.replace(/[←→]/g, '').matchAll(TAB)].map((m) => ({ label: m[2].trim(), done: m[1] === '☒' || m[1] === '■' || m[1] === '◼', current: false }));
  if (!out.length) return undefined;
  const now = review ? out.length - 1 : out.findIndex((t, i) => !t.done && i < out.length - 1);
  if (now >= 0) out[now].current = true;
  return out;
}

function details(lines: string[], from: number, to: number): string | undefined {
  const out: string[] = [];
  for (let i = from + 1; i < to; i++) {
    const t = lines[i].trim();
    if (!t || RULE.test(t) || SUBMIT.test(t) || !/^\s{2,}/.test(lines[i])) break;
    out.push(t);
  }
  return out.length ? out.join(' ') : undefined;
}

function numbered(lines: string[], agent: string): PromptCard | null {
  const hits: { i: number; n: number; text: string; cursor: boolean }[] = [];
  lines.forEach((l, i) => {
    const t = l.trim();
    const m = t.match(NUMBERED);
    if (m) hits.push({ i, n: Number(m[1]), text: m[2].trim(), cursor: MARK.test(t) });
  });
  if (!hits.length) return null;
  let end = hits.length - 1;
  let start = end;
  while (start > 0 && hits[start - 1].n === hits[start].n - 1 && hits[start].i - hits[start - 1].i <= 6) start--;
  const group = hits.slice(start, end + 1);
  if (group[0].n !== 1 || group.length < 2) return null;
  const last = group[group.length - 1];
  const chat = group.length > 2 && (/^Chat about this$/.test(last.text.replace(MARK, '')) || lines.slice(group[group.length - 2].i + 1, last.i).some((l) => l.trim() && RULE.test(l.trim()))) ? last : null;
  const multi = group.some((g) => CHECK.test(g.text));
  let submitAt = -1;
  for (let i = last.i + 1; i < Math.min(lines.length, last.i + 8); i++) {
    if (SUBMIT.test(lines[i].trim())) {
      submitAt = i;
      break;
    }
  }
  const field = chat ? group[group.length - 2] : multi ? last : group.find((g) => TYPE.test(g.text.replace(CHECK, '').replace(MARK, '')));
  const review = above(lines, group[0].i, /Review your answers/);
  const head = tabs(lines, group[0].i, review >= 0);
  const ask = !!(chat || field || head || submitAt >= 0);
  const at = group.find((g) => g.cursor);
  const submitN = last.n + 1;
  const cursor = at ? at.n : submitAt >= 0 && MARK.test(lines[submitAt].trim()) ? submitN : group[0].n;
  const cursorIdx = Math.max(0, group.findIndex((g) => g.cursor));
  const options = group.map((g, j) => {
    const box = g.text.match(CHECK);
    const text = g.text.replace(CHECK, '');
    const next = j + 1 < group.length ? group[j + 1].i : submitAt >= 0 ? submitAt : lines.length;
    const o: PromptOption = { label: `${g.n}. ${label(text)}`, keys: [] };
    const sc = shortcut(g.text);
    if (ask) {
      o.keys = g === field ? moves(cursor, g.n) : multi ? [...moves(cursor, g.n), 'space'] : arrows(cursor, g.n);
      const d = details(lines, g.i, next);
      if (d) o.description = d;
      if (g === field) o.input = true;
      if (g === chat) o.chat = true;
      if (box) o.checked = box[1] !== ' ';
    } else o.keys = agent === 'codex' && sc ? [sc] : agent === 'codex' && !sc ? arrows(cursorIdx, j) : [String(g.n)];
    return o;
  });
  const question = review >= 0 ? lines.slice(review, group[0].i).map((l) => l.trim()).filter((t) => t && !RULE.test(t)).join('\n') : questionAbove(lines, group[0].i);
  const card: PromptCard = { question, options };
  if (head) card.tabs = head;
  if (submitAt >= 0) card.submit = { label: lines[submitAt].trim().match(SUBMIT)![1], keys: arrows(cursor, submitN) };
  if (ask && lines.slice(last.i).some((l) => /esc to cancel/i.test(l))) card.cancel = true;
  return card;
}

function cursorMenu(lines: string[]): PromptCard | null {
  for (let i = lines.length - 1; i >= 0; i--) {
    const raw = lines[i];
    const m = raw.match(/^(\s*)([❯›▶])\s+(\S.*)$/);
    if (!m) continue;
    const col = raw.length - m[3].length;
    const sibling = (l: string | undefined) => l !== undefined && l.trim() !== '' && l.length > col && !l.slice(0, col).trim() && l[col] !== ' ';
    let a = i;
    let b = i;
    while (sibling(lines[a - 1])) a--;
    while (sibling(lines[b + 1])) b++;
    if (a === b) return null;
    const options = lines.slice(a, b + 1).map((l, j) => ({ label: label(j + a === i ? m[3] : l.trim()), keys: arrows(i - a, j) }));
    return { question: questionAbove(lines, a), options };
  }
  return null;
}

function letters(lines: string[]): PromptCard | null {
  const opts: { i: number; label: string; key: string }[] = [];
  lines.forEach((l, i) => {
    const t = l.trim().replace(MARK, '');
    const sc = shortcut(t);
    if (sc && t.length < 120) opts.push({ i, label: label(t), key: sc });
  });
  if (opts.length < 2) return null;
  const last = opts.filter((o) => o.i >= opts[opts.length - 1].i - 6);
  if (last.length < 2) return null;
  return { question: questionAbove(lines, last[0].i), options: last.map((o) => ({ label: o.label, keys: [o.key] })) };
}

function yesNo(lines: string[]): PromptCard | null {
  for (let i = lines.length - 1; i >= Math.max(0, lines.length - 4); i--) {
    const t = lines[i].trim();
    if (!t) continue;
    if (/[[(]\s*y(es)?\s*\/\s*n(o)?\s*[\])]/i.test(t))
      return {
        question: t,
        options: [
          { label: 'Yes', keys: ['y', 'enter'] },
          { label: 'No', keys: ['n', 'enter'] },
        ],
      };
    return null;
  }
  return null;
}

export function parsePrompt(screen: string, agent: string): PromptCard | null {
  const lines = screen.replace(/\r/g, '').split('\n').map(clean);
  while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
  const tail = lines.slice(-40);
  return numbered(tail, agent) ?? letters(tail) ?? cursorMenu(tail) ?? yesNo(tail);
}

export const PICKER_PROBES_MS = [400, 800, 1300];

export function isPickerCommand(text: string): boolean {
  return /^\/[\w:.-]+$/.test(text.trim());
}

export function parsePicker(screen: string, agent: string): PromptCard | null {
  const tail = screen.replace(/\r/g, '').split('\n').filter((l) => l.trim()).slice(-3).join('\n');
  return /esc to (cancel|go back|close|exit)/i.test(tail) ? parsePrompt(screen, agent) : null;
}
