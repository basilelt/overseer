import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { AttentionEntry } from '../../attention';
import AttentionBanner from './AttentionBanner.svelte';

const e = (key: string, title: string, detail?: string): AttentionEntry => ({ key, title, detail, workspaceId: null, at: '2026-11-05T10:00:00Z' });
const html = (entries: AttentionEntry[]) => render(AttentionBanner, { props: { entries, onselect: () => {} } }).body;

describe('AttentionBanner', () => {
  it('renders nothing when no session needs attention', () => {
    // Given no entries
    // When the banner renders
    // Then nothing but the Svelte anchor is output
    expect(html([]).replace(/<!--.*?-->/g, '')).toBe('');
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
});
