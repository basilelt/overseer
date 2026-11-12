import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { AttentionEntry } from '../../attention';
import AttentionBanner from './AttentionBanner.svelte';
import componentSrc from './AttentionBanner.svelte?raw';

const e = (key: string, title: string, detail?: string): AttentionEntry => ({ key, title, detail, workspaceId: null, at: '2026-11-05T10:00:00Z' });
const html = (entries: AttentionEntry[], pathOf?: (e: AttentionEntry) => string | undefined) => render(AttentionBanner, { props: { entries, onselect: () => {}, pathOf } }).body;

describe('AttentionBanner', () => {
  it('renders nothing when no session needs attention', () => {
    expect(html([]).split(/<!--[\s\S]*?--!?>/).join('')).toBe('');
  });

  it('lists each session as its own button under an attention heading', () => {
    const h = html([e('line:a', 'Fix login', 'Asked a question'), e('line:b', 'Docs')]);
    expect(h).toContain('Requires attention');
    expect(h.match(/<button/g)).toHaveLength(2);
    expect(h).toContain('>Fix login<');
    expect(h).toContain('>Asked a question<');
    expect(h).toContain('>Docs<');
  });

  it('uses the attention token for the dots', () => {
    expect(html([e('line:a', 'A')])).toContain('var(--state-attention)');
  });

  it('opens the session on click with its own hover token', () => {
    expect(componentSrc).toMatch(/onclick=\{\(\) => onselect\(entry\)\}/);
    expect(componentSrc).toContain('var(--attention-hover)');
  });

  it('shows the folder path of the session workspace under its title when given one', () => {
    const h = html([e('line:a', 'Hermes'), e('line:b', 'Loose')], (x) => (x.key === 'line:a' ? 'Endurance › core › endurance-core' : undefined));
    expect(h).toContain('>Endurance › core › endurance-core<');
    expect(h.match(/class="path/g)).toHaveLength(1);
  });

  it('shows no path without a lookup', () => {
    expect(html([e('line:a', 'Hermes')])).not.toContain('class="path');
  });
});
