import { describe, expect, it, vi } from 'vitest';
import app from '../../App.svelte?raw';
import bar from '../components/workspace/TabBar.svelte?raw';
import graph from './GitGraphTab.svelte?raw';
import liveBar from './LiveBar.svelte?raw';
import rail from '../components/sidebar/SessionRail.svelte?raw';
import sidebar from '../components/sidebar/Sidebar.svelte?raw';
import sideGit from './SidebarGit.svelte?raw';
import sc from './SourceControl.svelte?raw';
import detail from './CommitDetail.svelte?raw';

describe('git button', () => {
  it('toggles the live review of its session instead of opening the review menu', () => {
    expect(bar).toMatch(/onclick=\{\(\) => void import\('\.\.\/\.\.\/git\/live-actions'\)\.then\(\(m\) => m\.toggleLiveReview\(pane\.active\)\)\}/);
    expect(bar).toMatch(/aria-pressed=\{liveOn\(\)\}/);
    expect(bar).not.toMatch(/reviewMenu|review\.beside|review\.asTab|setReviewMode/);
  });

  it('leaves the review modes to the palette', async () => {
    const { command } = await import('../commands/commands');
    const { registerMore } = await import('../commands/commands-more');
    registerMore();
    expect(command('review-beside').label!()).toBe('Review beside the agent');
    expect(command('review-close').section).toBe('actions');
    expect(command('live-review').label!()).toBe('Open or close the live review');
    expect(command('graph').section).toBe('views');
  });

  it('swaps the sidebar for the session rail, hides the side panel and adds the git bar, all loaded only when used', () => {
    expect(app).toMatch(/const live = \$derived\(liveOn\(\)\)/);
    expect(app).toMatch(/\{#if live\}\s*<Boundary name="rail">\{#await rail\(\) then \{ default: SessionRail \}\}<SessionRail \/>\{\/await\}<\/Boundary>\s*\{:else if app\.sidebarOpen\}/);
    expect(app).toMatch(/\{#if app\.panelOpen && !live && app\.view === 'agents'/);
    expect(app).toMatch(/\{#if live\}\s*<Boundary name="live">\{#await liveBar\(\) then \{ default: LiveBar \}\}<LiveBar \/>\{\/await\}<\/Boundary>/);
    expect(app).not.toMatch(/GitPanel/);
  });

  it('opens the Git tab on phone from More', async () => {
    const { app } = await import('../state.svelte');
    const open = vi.spyOn(app, 'openProjectTab').mockImplementation(() => {});
    const { command } = await import('../commands/commands');
    const { registerMore } = await import('../commands/commands-more');
    registerMore();
    command('graph').run();
    expect(open).toHaveBeenCalledWith('graph');
    open.mockRestore();
  });

});

describe('sidebar git tab', () => {
  it('sits next to Workspaces and By status, and swaps the session list for source control, loaded only when used', () => {
    expect(sidebar).toMatch(/aria-selected=\{!app\.sidebarGit && app\.sidebarMode === m\.id\}/);
    expect(sidebar).toMatch(/aria-selected=\{app\.sidebarGit\}[^>]*onclick=\{\(\) => app\.setSidebarGit\(true\)\}/);
    expect(sidebar).toMatch(/\{#if app\.sidebarGit\}\s*\{#await gitView\(\) then \{ default: SidebarGit \}\}<SidebarGit \/>\{\/await\}/);
    expect(sidebar).toMatch(/const gitView = \(\) => import\('\.\.\/\.\.\/git\/SidebarGit\.svelte'\)/);
    expect(sidebar).toMatch(/\{#if !app\.sidebarGit\}\s*<div class="tools">\s*<label class="filter">/);
  });

  it('follows the selected session, lets you pick another worktree or submodule, and resets the pick on a new session', () => {
    expect(sideGit).toMatch(/const session = \$derived\(itemTarget\(app\.selectedKey\)\)/);
    expect(sideGit).toMatch(/const target = \$derived<ScmTarget \| null>\(picked \? \{ worktree: picked \} : session\)/);
    expect(sideGit).toMatch(/lastSession = k;\s*untrack\(\(\) => \(picked = null\)\);/);
    expect(sideGit).toMatch(/worktreeChoices\(related, subs, s\?\.worktree\)/);
    expect(sideGit).toMatch(/<SourceControl \{scm\} onopen=\{\(w\) => \(picked = w\)\} \/>/);
  });
});

describe('session rail', () => {
  it('lists the sessions of the workspace and opens one in the agent pane, not the review pane', () => {
    expect(rail).toMatch(/app\.scopeItems\(\)\.filter\(\(i\) => i\.lineId \|\| i\.termId\)/);
    expect(rail).toMatch(/findIndex\(\(p\) => !p\.tabs\.includes\(beside\.key \?\? ''\)\)/);
    expect(rail).toMatch(/update\(\(l\) => focusPane\(l, i\)\);\s*app\.select\(key\);/);
    expect(rail).toMatch(/aria-label=\{it\.title\}/);
  });
});

describe('git bar', () => {
  it('acts on the picked worktree, names it, and lists every worktree of the session with its submodules', () => {
    expect(liveBar).toMatch(/const target = \$derived\(liveTarget\(\)\)/);
    expect(liveBar).toMatch(/scm\.setTarget\(target\)/);
    expect(liveBar).toMatch(/\.related\(t\)/);
    expect(liveBar).toMatch(/worktreeChoices\(related, subs, s\?\.worktree\)/);
    expect(liveBar).toMatch(/<span class="nm">\{name\}<\/span>/);
  });

  it('shows the branch only when the worktree folder does not already say it', () => {
    expect(liveBar).toMatch(/\{#if !s\.branch \|\| s\.branch\.replaceAll\('\/', '-'\) !== name\}<span class="br mono">/);
  });

  it('forgets the picked worktree when the session changes', () => {
    expect(liveBar).toMatch(/if \(k === lastKey\) return;\s*lastKey = k;\s*untrack\(\(\) => \{\s*live\.worktree = null;/);
  });

  it('runs the action the state calls for, and commits on Enter', () => {
    expect(liveBar).toMatch(/const primary = \$derived\(primaryAction\(s, message\)\)/);
    expect(liveBar).toMatch(/if \(k === 'commit' \|\| k === 'merge'\) commit\(\);\s*else if \(k === 'publish' \|\| k === 'push'\) scm\.push\(\);\s*else if \(k === 'pull'\) scm\.pull\(\);/);
    expect(liveBar).toMatch(/e\.key === 'Enter'/);
  });

  it('asks before discarding everything, with a Cancel that discards nothing', () => {
    expect(liveBar).toMatch(/label: gt\('live\.discardAll'\), danger: true, disabled: busy \|\| !total, run: \(\) => \(confirmAll = true\)/);
    expect(liveBar).toMatch(/<button class="btn ghost" onclick=\{\(\) => \(confirmAll = false\)\}>/);
    expect(liveBar).toMatch(/async function discardAll\(\) \{\s*confirmAll = false;\s*await scm\.discard\('all'\);/);
  });

  it('warns when the agent is still running in the worktree', () => {
    expect(liveBar).toMatch(/i\.lineId === session\.lineId && i\.state === 'running'/);
    expect(liveBar).toMatch(/\{:else if running && total\}/);
  });

  it('shows the files in a popover that reuses source control without its commit box or listeners', () => {
    expect(liveBar).toMatch(/<SourceControl \{scm\} filesOnly onopen=\{\(w\) => \{ live\.worktree = w; filesOpen = false; \}\} \/>/);
    expect(sc).toMatch(/if \(!filesOnly\) untrack\(\(\) => scm\.connection\(phase\)\);/);
    expect(sc).toMatch(/onMount\(\(\) => \{\s*if \(filesOnly\) return;/);
  });

  it('opens the Git graph on the worktree from the more menu', () => {
    expect(liveBar).toMatch(/app\.gitWorktree = \{ project: s\.project, path: s\.worktree \};\s*app\.openProjectTab\('graph', s\.project\);/);
  });
});

describe('submodules in source control', () => {
  it('stages a submodule only when its commit moved, and opens it otherwise', () => {
    expect(sc).toMatch(/\{#if f\.submodule\}/);
    expect(sc).toMatch(/\{#if f\.submodule\.commit\}<button class="btn ghost sm" onclick=\{\(\) => scm\.stage\(\[f\.path\]\)\}/);
    expect(sc).toMatch(/onclick=\{\(\) => onopen\(sub\.worktree\)\}/);
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
    expect(sc).toContain('isSubmitChord(e) && canCommit');
  });

  it('refreshes on events about its worktree, debounced, and stops listening on destroy', () => {
    expect(sc).toMatch(/if \(!touches\(scm\.status, e\)\) return;/);
    expect(sc).toMatch(/timer = setTimeout\(\(\) => scm\.load\(\), 300\)/);
    expect(sc).toMatch(/return \(\) => \{\s*off\(\);\s*clearTimeout\(timer\);/);
  });

  it('reloads its status when the connection comes back', () => {
    expect(sc).toMatch(/const phase = app\.conn\.phase;\s*if \(!filesOnly\) untrack\(\(\) => scm\.connection\(phase\)\);/);
    expect(liveBar).toMatch(/const phase = app\.conn\.phase;\s*untrack\(\(\) => scm\.connection\(phase\)\);/);
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

  it('switches to the worktree the git bar opened it from, once', () => {
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

  it('follows the layout live, so a tablet turning across the phone width switches layout', () => {
    expect(graph).toMatch(/const phone = \$derived\(isPhone\(\)\);/);
  });

  it('splits changes and graph into tabs on phone', () => {
    expect(graph).toMatch(/\{#if phone\}\s*<div class="ptabs" role="tablist">/);
    expect(graph).toMatch(/class:hidden=\{phone && phoneView !== 'graph'\}/);
  });
});
