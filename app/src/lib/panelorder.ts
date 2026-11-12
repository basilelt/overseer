export const PANEL_SECTIONS = ['last', 'log', 'todos', 'files', 'diff', 'cost'] as const;
export type PanelSection = (typeof PANEL_SECTIONS)[number];

export function normalizeOrder(order: readonly string[] | undefined): PanelSection[] {
  const known = (order ?? []).filter((s, i, a): s is PanelSection => (PANEL_SECTIONS as readonly string[]).includes(s) && a.indexOf(s) === i);
  return [...known, ...PANEL_SECTIONS.filter((s) => !known.includes(s))];
}

export function moveSection(order: readonly string[] | undefined, from: string, to: string, after: boolean): PanelSection[] {
  const list = normalizeOrder(order);
  if (from === to || !list.includes(from as PanelSection) || !list.includes(to as PanelSection)) return list;
  const rest = list.filter((s) => s !== from);
  rest.splice(rest.indexOf(to as PanelSection) + (after ? 1 : 0), 0, from as PanelSection);
  return rest;
}
