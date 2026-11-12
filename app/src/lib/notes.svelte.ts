export const notesUi = $state<{ lineId: string | null; x: number; y: number }>({ lineId: null, x: 0, y: 0 });

/** Opens the sticky notes of a session next to the control at `anchor`; the same control closes them again. */
export function toggleNotes(lineId: string, anchor: Pick<DOMRect, 'right' | 'top'>) {
  if (notesUi.lineId === lineId) return closeNotes();
  notesUi.lineId = lineId;
  notesUi.x = anchor.right + 6;
  notesUi.y = anchor.top;
}

export function closeNotes() {
  notesUi.lineId = null;
}
