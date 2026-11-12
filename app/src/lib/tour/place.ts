export interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

export function union(rects: Rect[]): Rect | null {
  const r = rects.filter((x) => x.width > 0 && x.height > 0);
  if (!r.length) return null;
  const top = Math.min(...r.map((x) => x.top));
  const left = Math.min(...r.map((x) => x.left));
  const bottom = Math.max(...r.map((x) => x.top + x.height));
  const right = Math.max(...r.map((x) => x.left + x.width));
  return { top, left, width: right - left, height: bottom - top };
}

export interface Placement {
  top: number;
  left: number;
}

export function placeCard(target: Rect, card: { width: number; height: number }, view: { width: number; height: number }, gap = 12, margin = 12): Placement {
  const clampX = (x: number) => Math.max(margin, Math.min(view.width - card.width - margin, x));
  const clampY = (y: number) => Math.max(margin, Math.min(view.height - card.height - margin, y));
  const below = target.top + target.height + gap;
  if (below + card.height + margin <= view.height) return { top: below, left: clampX(target.left) };
  const above = target.top - gap - card.height;
  if (above >= margin) return { top: above, left: clampX(target.left) };
  const right = target.left + target.width + gap;
  if (right + card.width + margin <= view.width) return { top: clampY(target.top), left: right };
  const left = target.left - gap - card.width;
  if (left >= margin) return { top: clampY(target.top), left };
  return { top: clampY(view.height - card.height - margin), left: clampX((view.width - card.width) / 2) };
}
