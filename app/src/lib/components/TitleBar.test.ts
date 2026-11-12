import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import TitleBar from './TitleBar.svelte';
import { hint } from '../commands/keyboard';
import titleBarSrc from './TitleBar.svelte?raw';

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

  it('shows no avatar strip of running agents', () => {
    expect(titleBarSrc).not.toContain('AgentAvatars');
    expect(Object.keys(import.meta.glob('./titlebar/AgentAvatars.svelte'))).toHaveLength(0);
  });

  it('loads the usage and system chips after startup, on idle, to keep the startup bundle small', () => {
    expect(titleBarSrc).toContain("const usageChip = preload(() => import('./usage/UsageChip.svelte'));");
    expect(titleBarSrc).toContain("const systemChip = preload(() => import('./system/SystemChip.svelte'));");
    expect(titleBarSrc).not.toMatch(/import (UsageChip|SystemChip) from/);
  });
});
