import { describe, expect, it } from 'vitest';
import { en } from '../../i18n/en';

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
});
