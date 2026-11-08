import { describe, expect, it } from 'vitest';
import source from './Overlays.svelte?raw';
import { en } from '../../i18n/en';

describe('uninstall dialog overlay', () => {
  it('returns to Settings when closed instead of dropping every overlay', () => {
    expect(source).toContain("<UninstallDialog onclose={() => app.open('settings')} />");
  });
});

describe('profile apply hint', () => {
  it('points to the backups folder, not a .bak next to the file', () => {
    expect(en['profile.confirmHint']).not.toContain('.bak');
    expect(en['profile.confirmHint']).toContain('backups folder');
  });
});
