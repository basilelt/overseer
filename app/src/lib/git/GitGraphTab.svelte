<script lang="ts">
  import { gesture } from '../links';
  import { onMount, tick, untrack } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Search from '@lucide/svelte/icons/search';
  import Tag from '@lucide/svelte/icons/tag';
  import Segmented from '../components/kanban/Segmented.svelte';
  import AgentGlyph from '../components/ui/AgentGlyph.svelte';
  import { app } from '../state.svelte';
  import { customColors, resolvedTheme } from '../settings.svelte';
  import { age } from '../time';
  import { gitApi } from './api';
  import CommitDetail from './CommitDetail.svelte';
  import { drawGraph, readTheme } from './draw';
  import { maxWidth } from './layout';
  import { EMPTY_GRAPH, GraphLoader, type GraphSnapshot } from './loader';
  import { gt } from './strings';
  import type { Overlay, RefsFilter } from './types';
  import { ROW_H, authors, dimmed, graphWidth, labelsBySha, needsMore, overlaysBySha, scrollToRow, searchMatches, stepMatch, visibleRange } from './view';

  let { projectPath }: { projectPath: string } = $props();

  let snap = $state.raw<GraphSnapshot>(EMPTY_GRAPH);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let refs = $state<RefsFilter>('all');
  let author = $state('');
  let query = $state('');
  let selected = $state(-1);
  let detail = $state<string | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let list: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let loader: GraphLoader | null = null;

  const commits = $derived(snap.commits);
  const page = $derived(snap.page);
  const range = $derived(visibleRange(commits.length, scrollTop, viewport));
  const lanes = $derived(maxWidth(snap.rows));
  const gw = $derived(graphWidth(lanes));
  const labels = $derived(page ? labelsBySha(page) : new Map());
  const marks = $derived(page ? overlaysBySha(page.overlays) : new Map<string, Overlay[]>());
  const people = $derived(authors(commits));
  const matches = $derived(searchMatches(commits, query));
  const matchPos = $derived(matches.indexOf(selected));
  const headIndex = $derived(page ? commits.findIndex((c) => c.sha === page.head.sha) : -1);
  const visible = $derived(commits.slice(range.start, range.end).map((c, k) => ({ c, i: range.start + k })));

  async function load(keep = false) {
    if (!app.client) return;
    const sha = commits[selected]?.sha;
    loading = !keep;
    error = null;
    try {
      if (!loader || loader.project !== projectPath || loader.refs !== refs) {
        loader = new GraphLoader(gitApi(app.client), projectPath, refs);
        snap = await loader.load();
      } else {
        snap = await loader.reload();
      }
      if (keep && sha) selected = snap.commits.findIndex((c) => c.sha === sha);
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      loading = false;
    }
  }

  async function more() {
    if (!loader) return;
    try {
      snap = await loader.more();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  $effect(() => {
    projectPath;
    refs;
    untrack(() => {
      selected = -1;
      detail = null;
      if (list) list.scrollTop = 0;
      scrollTop = 0;
      load();
    });
  });

  $effect(() => {
    if (needsMore(range, commits.length, !!snap.next)) untrack(more);
  });

  $effect(() => {
    resolvedTheme();
    customColors();
    const rows = snap.rows;
    const { start, end } = range;
    const width = gw;
    const head = headIndex;
    const author_ = author;
    const cv = canvas;
    if (!cv) return;
    const dpr = globalThis.devicePixelRatio || 1;
    const h = Math.max(1, (end - start) * ROW_H);
    cv.width = Math.ceil(width * dpr);
    cv.height = Math.ceil(h * dpr);
    cv.style.width = `${width}px`;
    cv.style.height = `${h}px`;
    const ctx = cv.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const cs = commits;
    drawGraph(ctx, rows, start, end, readTheme(), {
      width,
      head,
      merges: (i) => cs[i].parents.length > 1,
      dimmed: author_ ? (i) => dimmed(cs[i], author_) : undefined,
    });
  });

  onMount(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const off = app.onChange((_p, _n, e) => {
      if (!/^(git|worktree)\./.test(e.type)) return;
      const p = e.data?.project ?? e.data?.worktree?.project;
      if (p && p !== projectPath) return;
      clearTimeout(timer);
      timer = setTimeout(() => load(true), 300);
    });
    return () => {
      off();
      clearTimeout(timer);
    };
  });

  async function select(i: number, open = false) {
    if (i < 0 || i >= commits.length) return;
    selected = i;
    if (open || detail) detail = commits[i].sha;
    await tick();
    if (list) list.scrollTop = scrollToRow(i, list.scrollTop, list.clientHeight);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'j') select(Math.min(commits.length - 1, selected + 1));
    else if (e.key === 'ArrowUp' || e.key === 'k') select(Math.max(0, selected - 1));
    else if (e.key === 'Enter' && selected >= 0) select(selected, true);
    else if (e.key === 'Escape' && detail) detail = null;
    else return;
    e.preventDefault();
  }

  function jump(dir: 1 | -1) {
    const i = stepMatch(matches, selected, dir);
    if (i >= 0) select(i, !!detail);
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    jump(e.shiftKey ? -1 : 1);
  }

  function highlight(text: string): [string, boolean][] {
    const q = query.trim();
    if (!q) return [[text, false]];
    const i = text.toLowerCase().indexOf(q.toLowerCase());
    if (i < 0) return [[text, false]];
    return [
      [text.slice(0, i), false],
      [text.slice(i, i + q.length), true],
      [text.slice(i + q.length), false],
    ];
  }

  const lineStatus = (o: Overlay) => (o.needsInput ? 'input' : o.busy ? 'working' : o.state === 'held' ? 'held' : 'idle');
  const lineTitle = (o: Overlay) => `${o.agent ?? ''} · ${o.needsInput ? gt('needsInput') : o.busy ? gt('working') : gt('idleLine')} · ${o.branch}`;
  const prClass = (s: string) => s.toLowerCase();
</script>

<section class="graph" aria-label={gt('graph')}>
  <div class="toolbar">
    <div class="refs">
      <Segmented
        value={refs}
        label={gt('refsLabel')}
        options={[
          { value: 'all', label: gt('refs.all') },
          { value: 'current', label: gt('refs.current') },
          { value: 'agents', label: gt('refs.agents') },
        ]}
        onchange={(v) => (refs = v)}
      />
    </div>
    <select class="author" bind:value={author} aria-label={gt('colAuthor')}>
      <option value="">{gt('allAuthors')}</option>
      {#each people as p (p)}<option value={p}>{p}</option>{/each}
    </select>
    <label class="search">
      <Search size={13} />
      <input placeholder={gt('search')} bind:value={query} onkeydown={onSearchKey} spellcheck="false" />
      {#if query.trim()}
        <span class="count mono">{matches.length ? gt('matches', { i: matchPos + 1 || '–', n: matches.length }) : gt('noMatch')}</span>
        <button class="icon-btn sm" title={gt('prevMatch')} aria-label={gt('prevMatch')} onclick={() => jump(-1)}><ChevronUp size={13} /></button>
        <button class="icon-btn sm" title={gt('nextMatch')} aria-label={gt('nextMatch')} onclick={() => jump(1)}><ChevronDown size={13} /></button>
      {/if}
    </label>
    <span class="spacer"></span>
    <span class="faint total">{snap.next ? gt('moreCommits', { n: commits.length.toLocaleString() }) : gt('commits', { n: commits.length.toLocaleString() })}</span>
    <button class="icon-btn" title={gt('refresh')} aria-label={gt('refresh')} onclick={() => load(true)}><RefreshCw size={14} /></button>
  </div>

  <div class="body">
    <div class="table">
      <div class="head" style:--gw="{gw}px">
        <span class="c-graph">{gt('colGraph')}</span>
        <span class="c-subject">{gt('colSubject')}</span>
        <span class="c-author">{gt('colAuthor')}</span>
        <span class="c-date">{gt('colDate')}</span>
        <span class="c-sha">{gt('colSha')}</span>
      </div>
      <div
        class="scroll"
        role="listbox"
        tabindex="0"
        aria-label={gt('graph')}
        aria-activedescendant={selected >= 0 ? `c-${commits[selected]?.sha}` : undefined}
        bind:this={list}
        bind:clientHeight={viewport}
        onscroll={(e) => (scrollTop = (e.currentTarget as HTMLElement).scrollTop)}
        onkeydown={onKey}
      >
        {#if loading && !commits.length}
          <p class="faint pad">{gt('loading')}</p>
        {:else if error}
          <p class="err pad">{error}</p>
        {:else if !commits.length}
          <p class="faint pad">{gt('empty')}</p>
        {/if}
        <div class="spacer-h" style:height="{commits.length * ROW_H}px">
          <div class="win" style:transform="translateY({range.start * ROW_H}px)" style:--gw="{gw}px">
            <canvas bind:this={canvas} class="lanes" aria-hidden="true"></canvas>
            {#each visible as { c, i } (c.sha)}
              {@const ls = labels.get(c.sha)}
              {@const os = marks.get(c.sha)}
              <div
                id="c-{c.sha}"
                class="row"
                class:sel={i === selected}
                class:dim={author && dimmed(c, author)}
                class:hit={query.trim() && i === matches[matchPos]}
                role="option"
                tabindex="-1"
                aria-selected={i === selected}
                onclick={() => select(i, true)}
                onkeydown={() => {}}
              >
                <span class="c-graph"></span>
                <span class="c-subject">
                  {#if os}
                    {#each os as o (o.branch)}
                      {#if o.agent}
                        <span class="agent" title={lineTitle(o)}>
                          <AgentGlyph agent={o.agent} size={12} />
                          <span class="st {lineStatus(o)}"></span>
                          {#if o.taskNumber}<span class="mono">#{o.taskNumber}</span>{/if}
                        </span>
                      {/if}
                      {#if o.ticketKey}<span class="ticket mono">{o.ticketKey}</span>{/if}
                      {#if o.pr}<a class="pr {prClass(o.pr.state)}" href={o.pr.url} onclick={(e) => { e.preventDefault(); e.stopPropagation(); app.openUrl(o.pr!.url, undefined, gesture(e)); }}>{gt('pr', { n: o.pr.number })}{o.pr.isDraft ? ' · draft' : ''}</a>{/if}
                    {/each}
                  {/if}
                  {#if ls}
                    {#each ls as l (l.kind + l.name)}
                      <span class="ref k-{l.kind}">{#if l.kind === 'tag'}<Tag size={10} />{/if}{l.name}</span>
                    {/each}
                  {/if}
                  <span class="subj">{#each highlight(c.subject) as [s, m], k (k)}{#if m}<mark>{s}</mark>{:else}{s}{/if}{/each}</span>
                </span>
                <span class="c-author">{c.author}</span>
                <span class="c-date" title={new Date(c.at).toLocaleString()}>{age(c.at)}</span>
                <span class="c-sha mono">{#each highlight(c.sha.slice(0, 7)) as [s, m], k (k)}{#if m}<mark>{s}</mark>{:else}{s}{/if}{/each}</span>
              </div>
            {/each}
          </div>
        </div>
      </div>
    </div>
    {#if detail && page}
      <CommitDetail project={projectPath} repo={page.repo} sha={detail} onclose={() => (detail = null)} onselect={(s) => select(commits.findIndex((c) => c.sha === s), true)} />
    {/if}
  </div>
</section>

<style>
  .graph {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .refs {
    flex: 0 0 auto;
    white-space: nowrap;
  }
  .author {
    height: 28px;
    max-width: 180px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 12px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 6px 0 8px;
    flex: 0 1 300px;
    min-width: 140px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-3);
  }
  .search:focus-within {
    border-color: var(--border-strong);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: none;
    font-size: 12px;
    color: var(--text);
  }
  .count {
    font-size: 11px;
    color: var(--text-3);
  }
  .icon-btn.sm {
    width: 20px;
    height: 20px;
  }
  .spacer {
    flex: 1;
  }
  .total {
    font-size: 12px;
    white-space: nowrap;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .table {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: var(--gw) minmax(0, 1fr) 150px 64px 74px;
    align-items: center;
    column-gap: 12px;
    padding-right: 12px;
  }
  .head {
    height: 28px;
    border-bottom: 1px solid var(--border);
    color: var(--text-3);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .head .c-graph {
    padding-left: 10px;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    outline: none;
    position: relative;
  }
  .spacer-h {
    position: relative;
  }
  .win {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    will-change: transform;
  }
  .lanes {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
    z-index: 1;
  }
  .row {
    height: 28px;
    cursor: default;
    font-size: 12.5px;
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row.hit {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row.dim > span:not(.c-graph) {
    opacity: 0.38;
  }
  .c-subject {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .subj {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .c-author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }
  .c-date {
    color: var(--text-3);
    white-space: nowrap;
  }
  .c-sha {
    color: var(--text-3);
    font-size: 11.5px;
  }
  mark {
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 2px;
  }
  .ref {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 18px;
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text-2);
    font-size: 11px;
    font-weight: 500;
  }
  .ref.k-head {
    border-color: var(--accent);
    color: var(--accent);
  }
  .ref.k-remote {
    border-style: dashed;
    color: var(--text-3);
    font-weight: 400;
  }
  .ref.k-tag {
    border-color: transparent;
    background: var(--surface-2);
  }
  .agent {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 18px;
    padding: 0 6px 0 5px;
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 11px;
  }
  .st {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--st-done);
  }
  .st.input {
    background: var(--st-input);
  }
  .st.working {
    background: var(--st-working);
  }
  .st.held {
    background: var(--st-held);
  }
  .ticket {
    flex: none;
    font-size: 11px;
    color: var(--text-2);
  }
  .pr {
    flex: none;
    font-size: 11px;
    color: var(--text-2);
    text-decoration: none;
    padding: 0 5px;
    height: 18px;
    line-height: 17px;
    border-radius: 4px;
    border: 1px solid var(--border);
  }
  .pr.merged {
    color: var(--st-testing);
  }
  .pr.closed {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .pad {
    padding: 16px;
    margin: 0;
  }
  .err {
    color: var(--st-failed);
  }
</style>
