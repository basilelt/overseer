import { describe, expect, it } from 'vitest';
import { en } from '../../i18n/en';
import { parentOverlay } from '../../overlay-route';

describe('uninstall dialog overlay', () => {
  it('returns to Settings when closed instead of dropping every overlay', () => {
    expect(parentOverlay('uninstall')).toBe('settings');
  });
});

describe('profile apply hint', () => {
  it('points to the backups folder, not a .bak next to the file', () => {
    expect(en['profile.confirmHint']).not.toContain('.bak');
    expect(en['profile.confirmHint']).toContain('backups folder');
  });
});

describe('closing the uninstall overlay', () => {
  it('goes back to Settings from the uninstall dialog and closes anything else', async () => {
    const { app } = await import('../../state.svelte');
    app.open('uninstall');
    app.closeOverlay();
    expect(app.overlay).toBe('settings');
    app.closeOverlay();
    expect(app.overlay).toBeNull();
  });

  it('routes Escape and the phone back button through closeOverlay', async () => {
    const appSrc = (await import('../../../App.svelte?raw')).default;
    const shell = (await import('../mobile/MobileShell.svelte?raw')).default;
    expect(appSrc).toMatch(/Escape' && app\.overlay\) \{\s*app\.closeOverlay\(\);/);
    expect(shell).toContain("action === 'overlay') app.closeOverlay();");
  });
});
