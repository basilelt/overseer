import { describe, expect, it } from 'vitest';
import titleBarSrc from './TitleBar.svelte?raw';

describe('TitleBar', () => {
  it('shows no avatar strip of running agents', () => {
    // Given the title bar source
    // When it is read
    // Then it renders no per-agent avatar buttons, which only duplicated the sidebar
    expect(titleBarSrc).not.toContain('AgentAvatars');
    expect(Object.keys(import.meta.glob('./titlebar/AgentAvatars.svelte'))).toHaveLength(0);
  });
});
