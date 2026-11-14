import { describe, expect, it } from 'vitest';
import { placeCard, union } from './place';

describe('tour geometry', () => {
  it('unions the visible rects', () => {
    expect(
      union([
        { top: 10, left: 100, width: 20, height: 10 },
        { top: 8, left: 130, width: 30, height: 14 },
        { top: 0, left: 0, width: 0, height: 0 },
      ]),
    ).toEqual({ top: 8, left: 100, width: 60, height: 14 });
    expect(union([])).toBeNull();
  });

  const view = { width: 1200, height: 800 };
  const card = { width: 320, height: 150 };

  it('places the card below the target when it fits', () => {
    expect(placeCard({ top: 10, left: 1100, width: 80, height: 24 }, card, view)).toEqual({ top: 46, left: 868 });
  });

  it('places the card above a target near the bottom', () => {
    expect(placeCard({ top: 700, left: 40, width: 80, height: 40 }, card, view)).toEqual({ top: 538, left: 40 });
  });

  it('places the card beside a tall target', () => {
    expect(placeCard({ top: 40, left: 0, width: 260, height: 760 }, card, view)).toEqual({ top: 40, left: 272 });
  });
});
