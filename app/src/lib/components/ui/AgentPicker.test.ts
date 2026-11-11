import { describe, expect, it } from 'vitest';
import picker from './AgentPicker.svelte?raw';

describe('agent picker', () => {
  it('installs an absent agent with an install command when it is chosen', () => {
    expect(picker).toMatch(/function choose[^{]*\{\s*if \(a && canInstall\(a\)\) return install\(a\);/);
    expect(picker).toContain('app.installAgent(a.id)');
    expect(picker).toContain('aria-disabled={!a.installed && !canInstall(a)}');
  });

  it('lets the keyboard reach installable agents', () => {
    expect(picker).toContain('if (agents[j].installed || canInstall(agents[j])) return j;');
  });
});
