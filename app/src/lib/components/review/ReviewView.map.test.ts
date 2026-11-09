import { describe, expect, it } from 'vitest';
import view from './ReviewView.svelte?raw';
import map from './MapView.svelte?raw';
import band from './HeatBand.svelte?raw';

describe('review map wiring', () => {
  it('offers List and Map, with the tour anchored on the switch, and no graph view left', () => {
    expect(view).toMatch(/\{ value: 'map' as const, label: t\('review\.map'\) \}/);
    expect(view).toMatch(/<span class="vseg" data-tour="review-map">/);
    expect(view).not.toMatch(/GraphView|'graph'/);
  });

  it('asks the map for every worktree the review shows, keyed on their revisions', () => {
    expect(view).toMatch(/const also = shown\.map\(\(s\) => s\.worktree \?\? ''\)\.filter\(Boolean\);/);
    expect(view).toMatch(/\[diff\.worktree, diff\.revision, \.\.\.shown\.map\(\(s\) => `\$\{s\.worktree\}@\$\{s\.diff!\.revision\}`\)\]/);
  });

  it('walks the hottest files with J and K only while focus is inside this review, and a click focuses it', () => {
    expect(view).toMatch(/<svelte:window onkeydown=\{onKey\} \/>/);
    expect(view).toMatch(/if \(!root \|\| !el \|\| !root\.contains\(el\)\) return;/);
    expect(view).toMatch(/closest\('input, textarea, select, \[contenteditable="true"\]'\)/);
    expect(view).toMatch(/bind:this=\{root\} tabindex="-1"/);
  });

  it('opens a file of a submodule or worktree the list does not show as its own review', () => {
    expect(view).toMatch(/if \(!s\) \{\s*openReviewNear\(\{ worktree: wt \}, selfKey\);/);
  });

  it('shows the heat band along the list on desktop only, when there is more than one file', () => {
    expect(view).toMatch(/\{#if !phone && bands\.length > 1\}\s*<HeatBand/);
    expect(band).toMatch(/style:flex-grow=\{b\.share\}/);
  });

  it('puts the ranking first on a phone and links across repositories dashed', () => {
    expect(map).toMatch(/\.map\.phone \.side \{\s*order: -1;/);
    expect(map).toMatch(/stroke-dasharray=\{l\.cross \? '6 4' : undefined\}/);
    expect(map).toMatch(/ondblclick=\{\(\) => open\(s\)\}/);
  });
});
