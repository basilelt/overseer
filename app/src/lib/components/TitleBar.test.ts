import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import TitleBar from './TitleBar.svelte';
import { hint } from '../commands/keyboard';

const html = () => render(TitleBar).body;
const paletteButton = () => html().match(/<button[^>]*data-tour="palette"[^>]*>/)?.[0] ?? '';

describe('TitleBar', () => {
  it('labels the palette button as the command palette with its shortcut', () => {
    const b = paletteButton();
    expect(b).toContain('aria-label="Command palette"');
    expect(b).toContain(`title="Command palette (${hint('palette')})"`);
  });

  it('shows no button called History', () => {
    expect(html()).not.toMatch(/aria-label="History"/);
  });
});
