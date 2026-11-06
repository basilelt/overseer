import { createRawSnippet } from 'svelte';
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import WorkspaceFrame from './WorkspaceFrame.svelte';
import componentSrc from './WorkspaceFrame.svelte?raw';
import css from '../../../app.css?raw';

const snip = (s: string) => createRawSnippet(() => ({ render: () => `<i>${s}</i>` }));

describe('WorkspaceFrame', () => {
  it('draws the header line above the body', () => {
    // Given a header and a body
    // When the frame renders
    // Then the header comes first
    const h = render(WorkspaceFrame, { props: { header: snip('HEAD'), children: snip('BODY') } }).body;
    expect(h.indexOf('HEAD')).toBeGreaterThanOrEqual(0);
    expect(h.indexOf('HEAD')).toBeLessThan(h.indexOf('BODY'));
  });

  it('renders the right-side header content when given', () => {
    // Given an actions snippet
    // When the frame renders
    // Then it appears after the header
    const h = render(WorkspaceFrame, { props: { header: snip('HEAD'), actions: snip('ACT'), children: snip('BODY') } }).body;
    expect(h.indexOf('ACT')).toBeGreaterThan(h.indexOf('HEAD'));
    expect(render(WorkspaceFrame, { props: { header: snip('HEAD'), children: snip('BODY') } }).body).not.toContain('ACT');
  });

  it('takes its look from theme tokens, not fixed colours', () => {
    // Given the component source
    // When its styles are read
    // Then colours come from CSS variables and nothing animates
    const src = componentSrc;
    expect(src).toContain('var(--ws-frame-border)');
    expect(src).toContain('var(--ws-frame-bg)');
    expect(src).toContain('border-radius: 10px');
    expect(src).not.toMatch(/#[0-9a-f]{3,6}\b|transition|animation/i);
  });

  it('defines frame tokens for both themes', () => {
    // Given the stylesheet
    // When the themes are read
    // Then dark carries the frozen values and light has its own
        const dark = css.slice(css.indexOf(":root[data-theme='dark']"));
    expect(dark).toMatch(/--ws-frame-border: #4a4a55/);
    expect(dark).toMatch(/--ws-frame-bg: #1e1e24/);
    expect(dark).toMatch(/--session-row-bg: #111114/);
    const light = css.slice(0, css.indexOf(":root[data-theme='dark']"));
    expect(light).toMatch(/--ws-frame-border:/);
    expect(light).toMatch(/--session-row-bg:/);
  });
});
