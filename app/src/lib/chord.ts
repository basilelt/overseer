export interface ChordKey {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
}

export function isSubmitChord(e: ChordKey): boolean {
  return e.key === 'Enter' && !!(e.ctrlKey || e.metaKey);
}
