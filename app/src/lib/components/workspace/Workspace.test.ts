import { describe, expect, it } from 'vitest';
import source from './Workspace.svelte?raw';

describe('Workspace split drag', () => {
  it('restores terminal pointer input when the split drag is canceled', () => {
    expect(source).toMatch(/onpointercancel=\{endDrag\}/);
    expect(source).toMatch(/onblur=\{endDrag\}/);
    expect(source).toMatch(/function endDrag\(\) \{\s*dragging = false;/);
  });
});
