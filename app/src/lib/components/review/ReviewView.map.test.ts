import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { declOf } from '../../../test/css';
import type { Band } from '../../review/map.svelte';
import HeatBand from './HeatBand.svelte';
import map from './MapView.svelte?raw';

const bands = [
  { id: 'a', share: 3, heat: 0.9 },
  { id: 'b', share: 1, heat: 0.1 },
] as Band[];
const html = (current: string | null = null) => render(HeatBand, { props: { bands, current, names: (id: string) => `file ${id}`, onpick: () => {} } }).body;

const button = (h: string, id: string) => h.match(new RegExp(`<button[^>]*aria-label="file ${id}"[^>]*>`))?.[0] ?? '';

describe('HeatBand', () => {
  it('draws one button per file, sized by its share and named for screen readers', () => {
    const h = html();
    expect(h.match(/<button/g)).toHaveLength(2);
    expect(h).toContain('aria-label="file a"');
    expect(h).toContain('aria-label="file b"');
    expect(button(h, 'a')).toContain('flex-grow: 3');
    expect(button(h, 'b')).toContain('flex-grow: 1');
  });

  it('marks the file being reviewed and only that one', () => {
    const h = html('b');
    expect(button(h, 'b')).toMatch(/class="[^"]*\bon\b/);
    expect(button(h, 'a')).not.toMatch(/class="[^"]*\bon\b/);
  });
});

describe('CSS guard: review map', () => {
  it('fades every cell but the hovered, pinned or related ones while one is focused', () => {
    expect(declOf(map, '.canvas.focus .cell:not(.sel):not(.near)', 'opacity')).toBe('0.32');
  });

  it('puts the ranking first on a phone', () => {
    expect(declOf(map, '.map.phone .side', 'order')).toBe('-1');
  });
});
