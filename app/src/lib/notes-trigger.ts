export const isNotesTrigger = (el: Element | null, lineId: string): boolean => !!el?.closest?.(`[data-notes="${CSS.escape(lineId)}"]`);
