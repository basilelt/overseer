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
    expect(list).toMatch(/onclose\(false\);\s*try \{\s*await app\.newSession\(a\.id\);/);
  });
});
