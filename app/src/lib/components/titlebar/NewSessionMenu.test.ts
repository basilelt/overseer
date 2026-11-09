import { describe, expect, it } from 'vitest';
import menu from './NewSessionMenu.svelte?raw';

describe('new session menu', () => {
  it('installs an absent agent with an install command instead of disabling it', () => {
    expect(menu).toContain('disabled={!a.installed && !canInstall(a)}');
    expect(menu).toContain('a.installed ? app.newSession(a.id) : app.installAgent(a.id)');
  });
});

describe('install tour target', () => {
  it('marks the + that starts sessions on desktop and on the phone', async () => {
    const chips = (await import('../mobile/MobileChips.svelte?raw')).default;
    expect(menu).toContain('<div class="wrap" data-tour="new-session"');
    expect(chips).toMatch(/data-tour="new-session"[^>]*onclick=\{\(\) => app\.open\('palette'\)\}/);
  });
});
