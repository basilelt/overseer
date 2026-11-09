import { describe, expect, it } from 'vitest';
import menu from './NewSessionMenu.svelte?raw';

describe('new session menu', () => {
  it('installs an absent agent with an install command instead of disabling it', () => {
    expect(menu).toContain('disabled={!a.installed && !canInstall(a)}');
    expect(menu).toContain('a.installed ? app.newSession(a.id) : app.installAgent(a.id)');
  });
});
