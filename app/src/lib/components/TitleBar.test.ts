import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import TitleBar from './TitleBar.svelte';
import { hint } from '../keyboard';
import titleBarSrc from './TitleBar.svelte?raw';

const html = () => render(TitleBar).body;
const paletteButton = () => html().match(/<button[^>]*data-tour="palette"[^>]*>/)?.[0] ?? '';

describe('TitleBar', () => {
  it('labels the palette button as the command palette with its shortcut', () => {
    // Given the title bar
    // When it renders
    // Then the button that opens the palette says so and shows its shortcut
    const b = paletteButton();
    expect(b).toContain('aria-label="Command palette"');
    expect(b).toContain(`title="Command palette (${hint('palette')})"`);
  });

  it('shows no button called History', () => {
    // Given the title bar
    // When it renders
    // Then no control claims to be a history it does not show
    expect(html()).not.toMatch(/aria-label="History"/);
  });

  it('shows no avatar strip of running agents', () => {
    // Given the title bar source
    // When it is read
    // Then it renders no per-agent avatar buttons, which only duplicated the sidebar
    expect(titleBarSrc).not.toContain('AgentAvatars');
    expect(Object.keys(import.meta.glob('./titlebar/AgentAvatars.svelte'))).toHaveLength(0);
  });

  it('loads the usage and system chips after startup, on idle, to keep the startup bundle small', () => {
    // Given the title bar source
    // When it is read
    // Then both chips come from preloaded dynamic imports, not static ones
    expect(titleBarSrc).toContain("const usageChip = preload(() => import('./usage/UsageChip.svelte'));");
    expect(titleBarSrc).toContain("const systemChip = preload(() => import('./system/SystemChip.svelte'));");
    expect(titleBarSrc).not.toMatch(/import (UsageChip|SystemChip) from/);
  });
});
