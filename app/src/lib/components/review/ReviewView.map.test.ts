import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { declOf } from '../../../test/css';
import { ReviewMapStore, fid, score, type Band } from '../../review/map.svelte';
import type { ReviewMap } from '../../review/types';
import HeatBand from './HeatBand.svelte';
import MapView from './MapView.svelte';
import map from './MapView.svelte?raw';
import heatBand from './HeatBand.svelte?raw';

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

const file = (path: string, logic: number) => ({ path, status: 'modified', size: 100, add: logic, del: 0, logic, noise: 0, sig: 0, edits: 1, fails: 0, viewed: false, threads: 0, test: false });
const data: ReviewMap = {
  regions: [{ worktree: '/w/web', name: 'web', branch: 'feat/x', parent: null, error: null, truncated: false, changed: [file('src/hot.ts', 30), file('src/cold.ts', 2)], tree: [] }],
  links: [],
};
function mapHtml(current: string | null, phone = false) {
  const m = new ReviewMapStore(() => null);
  m.data = data;
  m.scored = score(data);
  return render(MapView, { props: { map: m, current, phone, onopen: () => {} } }).body;
}

describe('MapView selection', () => {
  it('shows how to pick a file and the keys, not the hottest file, when nothing is selected', () => {
    const h = mapHtml(null);
    expect(h).toContain('Pick a file to see');
    expect(h).toContain('Clear the selection');
    expect(h).not.toContain('Open the diff');
  });

  it('leaves the keys out on a phone, which has no keyboard', () => {
    const h = mapHtml(null, true);
    expect(h).toContain('Pick a file to see');
    expect(h).not.toContain('Clear the selection');
  });

  it('details the selected file and marks it in the ranking', () => {
    const h = mapHtml(fid(0, 'src/cold.ts'));
    expect(h).toContain('Open the diff');
    expect(h).not.toContain('Pick a file to see');
    const on = (h.match(/<button[^>]*data-pick=[^>]*>/g) ?? []).filter((b) => /class="[^"]*\bon\b/.test(b));
    expect(on).toHaveLength(1);
    expect(on[0]).toContain(`data-pick="${fid(0, 'src/cold.ts')}"`);
  });

  it('tags every ranked file for keyboard moves', () => {
    const h = mapHtml(null);
    expect(h).toContain('data-rank');
    expect(h.match(/data-pick=/g)).toHaveLength(2);
  });
});

describe('CSS guard: heat band', () => {
  it('is wide enough to aim at', () => {
    expect(declOf(heatBand, '.band', 'width')).toBe('16px');
  });

  it('shows the hovered file name beside the band, above the diffs', () => {
    expect(declOf(heatBand, '.tip', 'left')).toBe('calc(100% + 4px)');
    expect(declOf(heatBand, '.tip', 'pointer-events')).toBe('none');
  });
});

describe('CSS guard: review map', () => {
  it('outlines a hovered file without fading the rest', () => {
    expect(declOf(map, '.cell.hot.hov:not(.sel)', 'outline')).toBe('1.5px solid var(--text-2)');
  });

  it('fades every cell but the selected one and its neighbours while one is selected', () => {
    expect(declOf(map, '.canvas.focus .cell:not(.sel):not(.near)', 'opacity')).toBe('0.32');
  });

  it('puts the ranking first on a phone', () => {
    expect(declOf(map, '.map.phone .side', 'order')).toBe('-1');
  });
});
