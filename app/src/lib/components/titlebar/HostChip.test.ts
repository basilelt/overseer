import { describe, expect, it } from 'vitest';
import chip from './HostChip.svelte?raw';
import menu from './HostMenu.svelte?raw';
import bar from '../TitleBar.svelte?raw';

describe('title bar startup weight', () => {
  it('keeps only the host chip at startup and loads its menu when it opens', () => {
    expect(chip).toMatch(/const menu = preload\(\(\) => import\('\.\/HostMenu\.svelte'\)\);/);
    expect(chip).toMatch(/\{#if open\}\s*\{#await menu\(\) then \{ default: HostMenu \}\}<HostMenu \{tip\} onclose=\{\(\) => \(open = false\)\} \/>\{\/await\}/);
    expect(chip).not.toMatch(/recentHosts|disconnect/);
  });

  it('closes the host menu before switching or disconnecting', () => {
    expect(menu).toMatch(/function pick\(h: HostTarget\) \{\s*onclose\(\);\s*app\.connect\(h\);/);
    expect(menu).toMatch(/async function disconnect\(\) \{\s*onclose\(\);\s*await app\.disconnect\(\);/);
  });

  it('loads the demo pill only in demo mode', () => {
    expect(bar).toMatch(/const demoPill = isDemo\(\) \? \(\) => import\('\.\/ui\/DemoPill\.svelte'\) : null;/);
    expect(bar).toMatch(/\{#if demoPill\}\{#await demoPill\(\) then \{ default: DemoPill \}\}<DemoPill \/>\{\/await\}\{\/if\}/);
  });
});
