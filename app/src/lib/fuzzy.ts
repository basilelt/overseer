export interface Match {
  score: number;
  ranges: [number, number][];
}

export function fuzzy(query: string, text: string): Match | null {
  const q = query.trim().toLowerCase();
  if (!q) return { score: 0, ranges: [] };
  const s = text.toLowerCase();
  const exact = s.indexOf(q);
  if (exact >= 0) {
    const boundary = exact === 0 || /[\s\-_/.:#]/.test(s[exact - 1]);
    return { score: 1000 - exact + (boundary ? 200 : 0) - (s.length - q.length) * 0.1, ranges: [[exact, exact + q.length]] };
  }
  const ranges: [number, number][] = [];
  let score = 0;
  let qi = 0;
  let prev = -2;
  for (let i = 0; i < s.length && qi < q.length; i++) {
    if (s[i] !== q[qi]) continue;
    const boundary = i === 0 || /[\s\-_/.:#]/.test(s[i - 1]);
    score += 10 + (i === prev + 1 ? 15 : 0) + (boundary ? 20 : 0) - Math.min(i, 20) * 0.2;
    if (i === prev + 1 && ranges.length) ranges[ranges.length - 1][1] = i + 1;
    else ranges.push([i, i + 1]);
    prev = i;
    qi++;
  }
  return qi === q.length ? { score, ranges } : null;
}

export function highlight(text: string, ranges: [number, number][]): { text: string; hit: boolean }[] {
  const out: { text: string; hit: boolean }[] = [];
  let at = 0;
  for (const [a, b] of ranges) {
    if (a > at) out.push({ text: text.slice(at, a), hit: false });
    out.push({ text: text.slice(a, b), hit: true });
    at = b;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

export function rank<T>(query: string, items: T[], label: (t: T) => string): { item: T; match: Match }[] {
  const out: { item: T; match: Match }[] = [];
  for (const item of items) {
    const match = fuzzy(query, label(item));
    if (match) out.push({ item, match });
  }
  return query.trim() ? out.sort((a, b) => b.match.score - a.match.score) : out;
}
