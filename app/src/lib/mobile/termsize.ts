export type ClientKind = 'phone' | 'desktop';

export interface TermSize {
  cols: number;
  rows: number;
  sizedBy?: string;
  lineId?: string | null;
  agent?: string;
}

export function isAgentTerm(term: Pick<TermSize, 'lineId' | 'agent'> | undefined): boolean {
  return !!(term?.lineId || term?.agent);
}

export function canResize(client: ClientKind, agentTerm: boolean): boolean {
  return client === 'desktop' || !agentTerm;
}

export function sizedElsewhere(term: TermSize | undefined, mine: { cols: number; rows: number } | null, me: ClientKind): ClientKind | 'other' | null {
  if (!term || !mine) return null;
  if (!canResize(me, isAgentTerm(term))) return null;
  if (term.cols === mine.cols && term.rows === mine.rows) return null;
  if (term.sizedBy === me) return null;
  if (term.sizedBy === 'phone' || term.sizedBy === 'desktop') return term.sizedBy;
  return 'other';
}

export function shouldReassert(visible: boolean, focused: boolean, hint: ReturnType<typeof sizedElsewhere>, resizable = true): boolean {
  return resizable && visible && focused && hint !== null;
}

export function fitTo(term: { cols: number; rows: number; resize(cols: number, rows: number): void }, want: { cols: number; rows: number } | undefined): boolean {
  if (!want || (want.cols === term.cols && want.rows === term.rows)) return false;
  term.resize(want.cols, want.rows);
  return true;
}
