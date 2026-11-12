import { describe, expect, it } from 'vitest';
import button from './NewSessionMenu.svelte?raw';
import list from './NewSessionList.svelte?raw';

describe('new session menu', () => {
  it('keeps only the button at startup and loads the menu when it opens', () => {
    expect(button).toMatch(/const list = preload\(\(\) => import\('\.\/NewSessionList\.svelte'\)\);/);
    expect(button).toMatch(/\{#if open\}\s*\{#await list\(\) then \{ default: NewSessionList \}\}<NewSessionList onclose=\{close\} \/>\{\/await\}/);
    expect(button).not.toMatch(/AgentGlyph|ProfileSelect/);
  });

  it('focuses the first agent when it opens, walks with the arrows and gives focus back on Escape', () => {
    expect(list).toMatch(/onMount\(\(\) => \{\s*menu\?\.querySelector<HTMLElement>\('button:not\(\[disabled\]\)'\)\?\.focus\(\);/);
    expect(list).toMatch(/else if \(e\.key === 'Escape'\) onclose\(true\);/);
    expect(button).toMatch(/if \(refocus\) root\.querySelector<HTMLElement>\('\.icon-btn'\)\?\.focus\(\);/);
  });

  it('closes before starting the session so a failure shows as a toast, not a stuck menu', () => {
    expect(list).toMatch(/onclose\(false\);\s*try \{\s*await \(a\.installed \? app\.newSession\(a\.id\) : app\.installAgent\(a\.id\)\);/);
  });
});

describe('agents that are not installed', () => {
  it('lists the installed agents and folds the missing ones behind one row', () => {
    expect(list).toMatch(/\{#each split\.installed as a/);
    expect(list).toMatch(/\{#if split\.missing\.length\}/);
    expect(list).toMatch(/aria-expanded=\{more\} onclick=\{\(\) => \(more = !more\)\}/);
    expect(list).toMatch(/\{#if more\}\s*\{#each split\.missing as a/);
  });

  it('starts folded each time the menu opens, since the list mounts on open', () => {
    expect(list).toMatch(/let more = \$state\(false\);/);
    expect(button).toMatch(/\{#if open\}\s*\{#await list\(\)/);
  });
});

describe('installing from the new session menu', () => {
  it('installs an absent agent with an install command instead of disabling it', () => {
    expect(list).toContain('disabled={!canInstall(a)}');
    expect(list).toContain('a.installed ? app.newSession(a.id) : app.installAgent(a.id)');
  });

  it('marks the + that starts sessions on desktop and on the phone for the tour', async () => {
    const chips = (await import('../mobile/MobileChips.svelte?raw')).default;
    expect(button).toContain('<div class="wrap" data-tour="new-session"');
    expect(chips).toMatch(/data-tour="new-session"[^>]*onclick=\{\(\) => app\.open\('palette'\)\}/);
  });
});
