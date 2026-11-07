import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { AttentionEntry } from '../../attention';
import AttentionBanner from './AttentionBanner.svelte';
import componentSrc from './AttentionBanner.svelte?raw';

const e = (key: string, title: string, detail?: string): AttentionEntry => ({ key, title, detail, workspaceId: null, at: '2026-11-05T10:00:00Z' });
const html = (entries: AttentionEntry[], pathOf?: (e: AttentionEntry) => string | undefined) => render(AttentionBanner, { props: { entries, onselect: () => {}, pathOf } }).body;

describe('AttentionBanner', () => {
  it('renders nothing when no session needs attention', () => {
    // Given no entries
    // When the banner renders
    // Then nothing but the Svelte anchor is output
    expect(html([]).split(/<!--[\s\S]*?--!?>/).join('')).toBe('');
  });

  it('lists each session as its own button under an attention heading', () => {
    // Given two sessions waiting
    // When the banner renders
    // Then both titles are buttons and the heading is the attention label
    const h = html([e('line:a', 'Fix login', 'Asked a question'), e('line:b', 'Docs')]);
    expect(h).toContain('Requires attention');
    expect(h.match(/<button/g)).toHaveLength(2);
    expect(h).toContain('>Fix login<');
    expect(h).toContain('>Asked a question<');
    expect(h).toContain('>Docs<');
  });

  it('uses the attention token for the dots', () => {
    // Given one entry
    // When it renders
    // Then the dot reads the attention colour token
    expect(html([e('line:a', 'A')])).toContain('var(--state-attention)');
  });

  it('opens the session on click with its own hover token', () => {
    // Given the component source
    // When it is read
    // Then click forwards the entry and hover differs from the banner background
    expect(componentSrc).toMatch(/onclick=\{\(\) => onselect\(entry\)\}/);
    expect(componentSrc).toContain('var(--attention-hover)');
  });

  it('shows the folder path of the session workspace under its title when given one', () => {
    // Given a session in Endurance › core › endurance-core and one outside any workspace
    // When the banner renders with a path lookup
    // Then the path shows once, for the first one only
    const h = html([e('line:a', 'Hermes'), e('line:b', 'Loose')], (x) => (x.key === 'line:a' ? 'Endurance › core › endurance-core' : undefined));
    expect(h).toContain('>Endurance › core › endurance-core<');
    expect(h.match(/class="path/g)).toHaveLength(1);
  });

  it('shows no path without a lookup', () => {
    // Given a session and no path lookup, as in the by-state view
    // When the banner renders
    // Then no path line is output
    expect(html([e('line:a', 'Hermes')])).not.toContain('class="path');
  });
});
