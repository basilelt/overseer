import { describe, expect, it } from 'vitest';
import app from '../../App.svelte?raw';
import bar from '../components/workspace/TabBar.svelte?raw';
import palette from '../components/overlays/CommandPalette.svelte?raw';
import more from '../components/mobile/MobileMore.svelte?raw';
import graph from './GitGraphTab.svelte?raw';
import panel from './GitPanel.svelte?raw';
import sc from './SourceControl.svelte?raw';
import detail from './CommitDetail.svelte?raw';

describe('git button', () => {
  it('toggles source control instead of opening the review menu', () => {
    expect(bar).toMatch(/onclick=\{\(\) => app\.toggleGitPanel\(\)\}/);
    expect(bar).toMatch(/aria-pressed=\{app\.gitPanel && app\.panelOpen\}/);
    expect(bar).not.toMatch(/reviewMenu|review\.beside|review\.asTab|setReviewMode/);
  });

  it('leaves the review modes to the palette', () => {
    expect(palette).toMatch(/view\('review-beside', 'review\.beside'/);
    expect(palette).toMatch(/action\('review-close', 'review\.close'/);
    expect(palette).toMatch(/action\('git-panel', 'palette\.toggleGitPanel'/);
    expect(palette).toMatch(/view\('graph', 'palette\.openGit'/);
  });

  it('shows source control in the side panel slot, loaded only when used', () => {
    expect(app).toMatch(/\{#if app\.gitPanel\}\{#await gitPanel\(\) then \{ default: GitPanel \}\}<GitPanel \/>\{\/await\}\{:else\}<SidePanel \/>\{\/if\}/);
    expect(app).toMatch(/const gitPanel = \(\) => import\('\.\/lib\/git\/GitPanel\.svelte'\)/);
  });

  it('opens the Git tab on phone from More', () => {
    expect(more).toMatch(/icon: GitBranch, label: \(\) => t\('tabs\.gitGraph'\), run: \(\) => app\.openProjectTab\('graph'\)/);
  });
});

describe('quick panel', () => {
  it('follows the selected session and expands into the Git tab on its worktree', () => {
    expect(panel).toMatch(/itemTarget\(app\.selectedKey\)/);
    expect(panel).toMatch(/app\.gitWorktree = \{ project: s\.project, path: s\.worktree \}/);
    expect(panel).toMatch(/app\.openProjectTab\('graph', s\.project\)/);
    expect(panel).toMatch(/onclick=\{\(\) => \(app\.gitPanel = false\)\}/);
  });
});

describe('source control', () => {
  it('asks before discarding, with a Cancel that discards nothing', () => {
    expect(sc).toMatch(/onclick=\{\(\) => \(confirming = \{ files: 'all'/);
    expect(sc).toMatch(/onclick=\{\(\) => \(confirming = \{ files: \[f\.path\]/);
    expect(sc).not.toMatch(/onclick=\{\(\) => scm\.discard/);
    expect(sc).toMatch(/<button class="btn ghost" onclick=\{\(\) => \(confirming = null\)\}>/);
    expect(sc).toMatch(/async function discard\(\) \{\s*if \(!confirming\) return;\s*const files = confirming\.files;\s*confirming = null;\s*await scm\.discard\(files\);/);
  });

  it('commits everything when nothing is staged, and on Ctrl+Enter', () => {
    expect(sc).toMatch(/const all = o\.all \|\| !s\.staged\.length;/);
    expect(sc).toMatch(/e\.key === 'Enter' && \(e\.ctrlKey \|\| e\.metaKey\) && canCommit/);
  });

  it('refreshes on events about its worktree, debounced, and stops listening on destroy', () => {
    expect(sc).toMatch(/if \(!touches\(scm\.status, e\)\) return;/);
    expect(sc).toMatch(/timer = setTimeout\(\(\) => scm\.load\(\), 300\)/);
    expect(sc).toMatch(/return \(\) => \{\s*off\(\);\s*clearTimeout\(timer\);/);
  });

  it('reloads its status when the connection comes back', () => {
    expect(sc).toMatch(/const phase = app\.conn\.phase;\s*untrack\(\(\) => scm\.connection\(phase\)\);/);
  });

  it('shows the file actions on touch screens, where there is no hover', () => {
    expect(sc).toMatch(/@media \(hover: none\) \{\s*\.acts \{\s*opacity: 1;/);
  });
});

describe('git tab', () => {
  it('puts source control beside the graph, with a worktree picker and an uncommitted row', () => {
    expect(graph).toMatch(/<SourceControl \{scm\} \/>/);
    expect(graph).toMatch(/<select class="mono" bind:value=\{chosen\}>/);
    expect(graph).toMatch(/\{#if wip && scm\.status\}\s*<button class="wip"/);
    expect(graph).toMatch(/scm\.setTarget\(\{ worktree: chosen \}\)/);
  });

  it('switches to the worktree the quick panel expanded from, once', () => {
    expect(graph).toMatch(/if \(w\?\.project !== projectPath\) return;\s*chosen = w\.path;[\s\S]*?app\.gitWorktree = null;/);
  });

  it('acts on the selected commit from its detail: merge into the worktree branch and revert', () => {
    expect(graph).toMatch(/const r = await scm\.merge\(pick\.sha\);/);
    expect(graph).toMatch(/const r = await scm\.revert\(pick\.sha\);/);
    expect(graph).toMatch(/\{#snippet extra\(\)\}[\s\S]*?onclick=\{mergePick\}[\s\S]*?onclick=\{revertPick\}[\s\S]*?\{\/snippet\}/);
    expect(detail).toMatch(/<Cherry size=\{13\} \/>\{gt\('cherryPick'\)\}<\/button>\s*\{@render extra\?\.\(\)\}/);
  });

  it('hides the source control column while a commit is open, so the graph keeps its width', () => {
    expect(graph).toMatch(/\{#if phone \? phoneView === 'changes' : showScm && !detail\}/);
  });

  it('scrolls the changes as one page on phone and keeps descriptions readable in the graph', () => {
    expect(graph).toMatch(/\.scm\.full \{[^}]*overflow-y: auto;/);
    expect(graph).toMatch(/\.graph\.phone \.c-author,\s*\.graph\.phone \.c-sha \{\s*display: none;/);
    expect(graph).toMatch(/\.graph\.phone \.toolbar \{\s*flex-wrap: wrap;/);
  });

  it('splits changes and graph into tabs on phone', () => {
    expect(graph).toMatch(/\{#if phone\}\s*<div class="ptabs" role="tablist">/);
    expect(graph).toMatch(/class:hidden=\{phone && phoneView !== 'graph'\}/);
  });
});
