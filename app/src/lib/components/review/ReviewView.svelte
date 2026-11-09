<script lang="ts">
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import FolderX from '@lucide/svelte/icons/folder-x';
  import Lock from '@lucide/svelte/icons/lock';
  import X from '@lucide/svelte/icons/x';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import ArrowDownWideNarrow from '@lucide/svelte/icons/arrow-down-wide-narrow';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Link2 from '@lucide/svelte/icons/link-2';
  import { onMount, tick, untrack } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { reviewCounts } from '../../review/counts.svelte';
  import { totals, type Mode } from '../../review/rows';
  import { ReviewStore } from '../../review/store.svelte';
  import { LinkedReviews, fileLabel, parsePick, pickerGroups } from '../../review/linked.svelte';
  import { ReviewMapStore, band, fid, ranking, step, type Indicator } from '../../review/map.svelte';
  import { reviewApi } from '../../review/api';
  import { app } from '../../state.svelte';
  import { countByFile } from '../../review/threads';
  import { reviewKey } from '../../review/keys';
  import { openReview, openReviewNear } from '../../review/open';
  import { isSince } from '../../review/base';
  import BasePicker from './BasePicker.svelte';
  import Boxes from '@lucide/svelte/icons/boxes';
  import Pin from '@lucide/svelte/icons/pin';
  import PinOff from '@lucide/svelte/icons/pin-off';
  import { changedWorktrees, counts as countLabel, folder } from '../../review/scope';
  import { KINDS, TAG_COLOR, lowRiskKinds } from '../../review/triage';
  import type { ReviewCommit, ReviewTarget, SubmitKind } from '../../review/types';
  import MapView from './MapView.svelte';
  import HeatBand from './HeatBand.svelte';
  import Flame from '@lucide/svelte/icons/flame';
  import Commits from './Commits.svelte';
  import ReviewScope from './ReviewScope.svelte';
  import Segmented from '../kanban/Segmented.svelte';
  import FileDiff from './FileDiff.svelte';
  import FileTree from './FileTree.svelte';
  import SubmitBar from './SubmitBar.svelte';
  import { isPhone } from '../../mobile/ui.svelte';

  let {
    target,
    embedded = false,
    onsubmitted,
    pinned = false,
    onpin,
  }: { target: ReviewTarget; embedded?: boolean; onsubmitted?: (kind: SubmitKind) => void; pinned?: boolean; onpin?: () => void } = $props();

  const store = new ReviewStore(untrack(() => target));
  const linked = new LinkedReviews();
  let scroller: HTMLDivElement | undefined = $state();
  let folded = $state<Record<string, boolean>>({});

  onMount(() => {
    reviewCounts.ensure();
    const stop = store.start();
    return () => {
      stop();
      linked.stop();
      maps.stop();
    };
  });

  $effect(() => {
    const l = store.diff?.linked;
    untrack(() => linked.sync(l));
  });


  const diff = $derived(store.diff);
  const shown = $derived(linked.visible);
  const anyFiles = $derived(!!diff?.files.length || shown.some((s) => s.diff!.files.length));
  const sum = $derived(diff ? totals(diff.files) : null);
  const counts = $derived(countByFile(store.threads));
  const phone = $derived(isPhone());
  const selfKey = reviewKey(untrack(() => target));
  const others = $derived(changedWorktrees(store.related).filter((w) => !w.current));
  const onDefault = $derived(!!diff?.base && !!diff.branch && diff.base.endsWith(`/${diff.branch}`));
  const commits = $derived(diff?.commits ?? []);
  const commitsTitle = $derived(
    diff && commits.length
      ? onDefault
        ? t(commits.length === 1 ? 'review.unpushed1' : 'review.unpushed', { n: commits.length })
        : t(commits.length === 1 ? 'review.commits1' : 'review.commits', { n: commits.length, base: isSince(store.base) ? store.base!.replace('~1', '').slice(0, 7) : (diff.base ?? 'HEAD') })
      : '',
  );
  const emptyText = $derived(
    !diff ? '' : diff.base ? t('review.inSync', { branch: diff.branch ?? 'HEAD', base: isSince(store.base) ? store.base!.slice(0, 7) : diff.base }) : t('review.clean'),
  );

  const triage = $derived(diff?.triage);
  const lowKinds = $derived(lowRiskKinds(triage).slice(0, 3));
  const ranked = $derived(!!diff?.files.some((f) => f.risk !== undefined));
  let sortRisk = $state(false);
  const maps = new ReviewMapStore(() => (app.client ? reviewApi(app.client) : null));
  let ind = $state<Indicator>('priority');
  let hot = $state<string | null>(null);
  let root: HTMLDivElement | undefined = $state();
  const mapKey = $derived(diff && anyFiles ? [diff.worktree, diff.revision, ...shown.map((s) => `${s.worktree}@${s.diff!.revision}`)].join('|') : '');
  $effect(() => {
    const k = mapKey;
    if (!k || !diff) return;
    const also = shown.map((s) => s.worktree ?? '').filter(Boolean);
    const wt = diff.worktree;
    untrack(() => maps.request(k, { worktree: wt }, also));
  });
  const rank = $derived(ranking(maps.scored, ind));
  const regionOf = (wt: string | null | undefined) => maps.data?.regions.findIndex((r) => r.worktree === wt) ?? -1;
  const bandOrder = $derived(
    diff
      ? [...diff.files.map((f) => fid(regionOf(diff.worktree), f.path)), ...shown.flatMap((s) => s.diff!.files.map((f) => fid(regionOf(s.worktree), f.path)))]
      : [],
  );
  const bands = $derived(band(maps.scored, bandOrder, ind));
  const upNext = $derived(rank.length ? (step(rank, hot, 1) ?? null) : null);
  const nameOf = (id: string) => {
    const s = maps.scored.find((x) => x.id === id);
    return s ? `${s.region.name}/${s.file.path}` : id;
  };

  function setView(v: 'list' | 'map') {
    store.setView(v);
  }

  async function openFile(w: number, path: string) {
    const wt = maps.data?.regions[w]?.worktree;
    if (!wt) return;
    hot = fid(w, path);
    const s = wt === diff?.worktree ? store : shown.find((x) => x.worktree === wt);
    if (!s) {
      openReviewNear({ worktree: wt }, selfKey);
      return;
    }
    setView('list');
    await tick();
    if (s === store) select(path);
    else selectIn(s, path);
  }

  function walk(dir: 1 | -1) {
    const n = step(rank, hot, dir);
    if (n) openFile(n.w, n.file.path);
  }

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey || (e.key !== 'j' && e.key !== 'k')) return;
    const el = e.target as HTMLElement | null;
    if (!root || !el || !root.contains(el)) return;
    if (el?.closest('input, textarea, select, [contenteditable="true"]')) return;
    if (!rank.length) return;
    e.preventDefault();
    walk(e.key === 'j' ? 1 : -1);
  }

  function since(c: ReviewCommit) {
    store.setBase(`${c.sha}~1`);
  }

  function select(path: string) {
    store.selected = path;
    store.collapsed[path] = false;
    reveal(scroller?.querySelector<HTMLElement>(`:scope > [data-path="${CSS.escape(path)}"]`));
  }

  function selectIn(s: ReviewStore, path: string) {
    const wt = s.worktree ?? '';
    s.selected = path;
    s.collapsed[path] = false;
    folded[wt] = false;
    tick().then(() => reveal(scroller?.querySelector<HTMLElement>(`[data-wt="${CSS.escape(wt)}"] [data-path="${CSS.escape(path)}"]`)));
  }

  function pick(v: string) {
    const p = parsePick(v);
    const s = p.worktree === null ? null : shown.find((x) => x.worktree === p.worktree);
    if (s) selectIn(s, p.path);
    else select(p.path);
  }

  function reveal(el: HTMLElement | null | undefined) {
    if (el && scroller) scroller.scrollTo({ top: el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop - 8, behavior: 'smooth' });
  }

  let frame = 0;
  function onScroll() {
    if (!frame) frame = requestAnimationFrame(track);
  }

  function track() {
    frame = 0;
    if (!scroller || !diff) return;
    const top = scroller.scrollTop + 60;
    let cur: string | null = null;
    for (const el of scroller.querySelectorAll<HTMLElement>(':scope > [data-path]')) {
      if (el.offsetTop - scroller.offsetTop <= top) cur = el.dataset.path ?? null;
      else break;
    }
    if (cur && cur !== store.selected) store.selected = cur;
  }
</script>

<svelte:window onkeydown={onKey} />
<div class="review" class:embedded class:phone bind:this={root}>
  {#if !embedded}<ReviewScope {store} {selfKey} />{/if}
  <header class="head" class:hide={store.missing || store.notRepo}>
    <div class="ref">
      <GitBranch size={14} />
      {#if diff}<span class="faint cmp">{t('review.comparing')}</span>{/if}
      <span class="mono branch">{diff?.branch ?? '…'}</span>
      {#if diff}
        <ArrowLeft size={12} class="faint" />
        {#if store.base && isSince(store.base)}
          <button class="chip mono" title={t('review.resetBase', { base: diff.base ?? 'HEAD' })} onclick={() => store.resetSince()}>{t('review.sinceActive', { sha: store.base.replace('~1', '').slice(0, 7) })}<X size={11} /></button>
        {:else if store.readOnly}
          <span class="mono base">{diff.base ?? 'HEAD'}</span>
        {:else}
          <BasePicker {store} />
        {/if}
        {#if !phone}<span class="faint mb">{diff.base ? t('review.against', { base: diff.mergeBase?.slice(0, 7) ?? diff.base }) : t('review.uncommitted')}</span>{/if}
      {/if}
      {#if diff?.superproject}
        <button class="chip sub" title={diff.superproject} onclick={() => openReviewNear({ worktree: diff.superproject! }, selfKey)}><Boxes size={11} />{t('review.submoduleOf', { name: folder(diff.superproject) })}</button>
      {/if}
    </div>
    {#if sum && diff && diff.files.length}
      <div class="stats">
        <span class="muted">{t('review.filesChanged', { n: diff.files.length })}</span>
        <span class="add">+{sum.additions}</span>
        <span class="del">−{sum.deletions}</span>
        <span class="viewed" class:hide={phone || store.readOnly} title={t('review.viewed')}>
          <span class="meter"><span style:width="{diff.files.length ? (sum.viewed / diff.files.length) * 100 : 0}%"></span></span>
          {t('review.viewedCount', { n: sum.viewed, total: diff.files.length })}
        </span>
        {#if triage?.lowRiskPct && lowKinds.length}
          <span class="low" title={t('review.lowRiskTitle')}>{t('review.lowRisk', { pct: triage.lowRiskPct, kinds: lowKinds.map((k) => t(`review.tag.${k}`)).join('/') })}</span>
        {/if}
        {#if triage}
          <details class="legend">
            <summary>{t('review.legend')}</summary>
            <ul class="lpop">
              {#each KINDS as k (k)}
                <li><span class="sw" style:background={TAG_COLOR[k]}></span><b>{t(`review.tag.${k}`)}</b><span class="muted">{t(`review.tagHelp.${k}`)}</span></li>
              {/each}
              <li><span class="sw" style:background="var(--accent)"></span><b>{t('review.tag.sig')}</b><span class="muted">{t('review.tagHelp.sig')}</span></li>
            </ul>
          </details>
        {/if}
      </div>
    {/if}
    <span class="spacer"></span>
    {#if store.live && !store.readOnly}<button class="live" class:paused={!store.autoRefresh} title={t(store.autoRefresh ? 'review.live' : 'review.livePaused')} aria-pressed={store.autoRefresh} onclick={() => { const on = !store.autoRefresh; store.setAutoRefresh(on); linked.setAutoRefresh(on); }}><span class="dot"></span>{store.autoRefresh ? 'Live' : t('review.liveOff')}</button>{/if}
    {#if upNext}
      <button class="upnext" onclick={() => walk(1)} title={`${t('review.nextHot')}: ${upNext.region.name}/${upNext.file.path} · ${t('review.nextHotKeys')}`}><Flame size={12} /><span class="mono">{upNext.file.path.split('/').pop()}</span><span class="faint">{rank.indexOf(upNext) + 1}/{rank.length}</span><ChevronRight size={12} /></button>
    {/if}
    {#if anyFiles}
      <span class="vseg" data-tour="review-map"><Segmented
        value={store.view}
        label={t('review.view')}
        options={[
          { value: 'list' as const, label: t('review.list') },
          { value: 'map' as const, label: t('review.map') },
        ]}
        onchange={setView}
      /></span>
    {/if}
    {#if !phone && store.view === 'list'}
    <Segmented
      value={store.mode}
      label={t('review.unified')}
      options={[
        { value: 'unified' as Mode, label: t('review.unified') },
        { value: 'split' as Mode, label: t('review.split') },
      ]}
      onchange={(m) => store.setMode(m)}
    />
    {/if}
    {#if onpin}
      <button class="icon-btn" class:on={pinned} title={t(pinned ? 'review.unpin' : 'review.pin')} aria-label={t(pinned ? 'review.unpin' : 'review.pin')} aria-pressed={pinned} onclick={onpin}>{#if pinned}<PinOff size={14} />{:else}<Pin size={14} />{/if}</button>
    {/if}
    <button class="icon-btn refresh" class:stale={store.stale} title={t(store.stale ? 'review.stale' : 'common.refresh')} aria-label={t(store.stale ? 'review.stale' : 'common.refresh')} onclick={() => store.load()}><RefreshCw size={14} /></button>
  </header>
  {#if store.notRepo}
    <div class="state faint">{t('panel.notRepo')}</div>
  {:else if store.loading && !diff}
    <div class="state faint"><LoaderCircle size={14} class="spin" />{t('common.loading')}</div>
  {:else if store.missing}
    <div class="state missing">
      <FolderX size={28} />
      <strong>{t('review.missing')}</strong>
      <span class="mono faint path">{store.error.match(/folder (.+) no longer exists/)?.[1] ?? ''}</span>
      <span class="muted hint">{t('review.missingHint')}</span>
    </div>
  {:else if store.error && !diff}
    <div class="state err" role="alert">{store.error}<button class="btn" onclick={() => store.load()}><RefreshCw size={13} />{t('common.retry')}</button></div>
  {:else if diff}
    {#if store.view === 'map' && anyFiles}
      <MapView map={maps} bind:ind {phone} current={hot} onopen={openFile} />
    {:else}
    {#if phone && anyFiles}
      <div class="picker">
        <select aria-label={t('mobile.review.file')} value={store.selected ?? diff.files[0]?.path} onchange={(e) => pick(e.currentTarget.value)}>
          {#each diff.files as f (f.path)}<option value={f.path}>{fileLabel(f)}</option>{/each}
          {#each pickerGroups(shown) as g (g.worktree)}
            <optgroup label={g.label}>
              {#each g.options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
            </optgroup>
          {/each}
        </select>
        {#if !store.readOnly}<span class="faint tap">{t('mobile.review.tapLine')}</span>{/if}
      </div>
    {/if}
    <div class="main">
      {#if !phone && anyFiles}
        <aside class="side">
          <div class="stitle">
            {t('review.files')}
            {#if ranked}
              <button class="sort" class:on={sortRisk} aria-pressed={sortRisk} title={t('review.sortRisk')} aria-label={t('review.sortRisk')} onclick={() => (sortRisk = !sortRisk)}><ArrowDownWideNarrow size={13} /></button>
            {/if}
          </div>
          <FileTree files={diff.files} {counts} selected={store.selected} flat={sortRisk} onselect={select} />
          {#each shown as s (s.worktree)}
            <div class="stitle lgroup" title={s.worktree}><Link2 size={11} /><span>{folder(s.worktree ?? '')}</span></div>
            <FileTree files={s.diff!.files} counts={countByFile(s.threads)} selected={s.selected} flat={sortRisk} onselect={(p) => selectIn(s, p)} />
          {/each}
        </aside>
      {/if}
      {#if !phone && bands.length > 1}
        <HeatBand {bands} current={hot} names={nameOf} onpick={(id) => { const x = maps.scored.find((y) => y.id === id); if (x) openFile(x.w, x.file.path); }} />
      {/if}
      <div class="files" bind:this={scroller} onscroll={onScroll}>
        {#if diff.truncated}<p class="note faint">{t('review.truncated')}</p>{/if}
        {#if store.readOnly}<p class="ro muted"><Lock size={12} />{t('review.readOnly')}</p>{/if}
        {#if commits.length}
          {#key diff.head}<Commits title={commitsTitle} {commits} onsince={store.readOnly ? undefined : since} />{/key}
        {/if}
        {#each diff.files as f (f.path)}
          <FileDiff {store} file={f} />
        {:else}
          <div class="empty">
            <p class="muted">{emptyText}</p>
            {#if diff.recent?.length}
              <Commits title={t('review.recent')} commits={diff.recent} open={!phone} onsince={store.readOnly ? undefined : since} />
            {/if}
            {#if others.length && !embedded}
              <div class="others">
                <div class="stitle">{t('review.otherWorktrees')}</div>
                {#each others as w (w.path)}
                  <button class="wt" onclick={() => openReview({ worktree: w.path })}>
                    <span class="mono name">{w.branch || folder(w.path)}</span>
                    <span class="faint">{countLabel({ files: w.files, ahead: w.ahead })}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
        {#each shown as s (s.worktree)}
          {@const ld = s.diff!}
          {@const lt = totals(ld.files)}
          {@const wt = ld.worktree}
          <section class="linked" data-wt={wt}>
            <button class="lhead" aria-expanded={!folded[wt]} title={`${t('review.linked')}: ${wt}`} onclick={() => (folded[wt] = !folded[wt])}>
              <span class="chev" class:open={!folded[wt]}><ChevronRight size={13} /></span>
              <Link2 size={13} />
              <span class="mono lname">{folder(wt)}</span>
              {#if ld.branch}<span class="mono faint lbranch">{ld.branch}</span>{/if}
              <span class="spacer"></span>
              <span class="muted">{t('review.filesChanged', { n: ld.files.length })}</span>
              <span class="add">+{lt.additions}</span>
              <span class="del">−{lt.deletions}</span>
            </button>
            {#if !folded[wt]}
              {#each ld.files as f (f.path)}
                <FileDiff store={s} file={f} />
              {/each}
            {/if}
          </section>
        {/each}
      </div>
    </div>
    {/if}
    {#if !store.readOnly}<SubmitBar {store} linked={shown} approve={!embedded} {onsubmitted} />{/if}
  {/if}
</div>

<style>
  .review {
    flex: 1;
    min-height: 0;
    min-width: 0;
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--text);
  }
  .head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 44px;
    padding: 0 12px 0 14px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    min-width: 0;
  }
  .ref {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--text-2);
    white-space: nowrap;
  }
  .branch {
    color: var(--text);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .base {
    color: var(--text);
  }
  .cmp {
    flex: none;
  }
  @container (max-width: 1240px) {
    .head {
      flex-wrap: wrap;
      height: auto;
      min-height: 44px;
      row-gap: 6px;
      padding-top: 6px;
      padding-bottom: 6px;
    }
    .ref {
      flex: 1 1 100%;
    }
  }
  @container (max-width: 820px) {
    .mb {
      display: none;
    }
  }
  @container (max-width: 640px) {
    .side {
      display: none;
    }
  }
  .ref .chip.sub {
    background: var(--surface-active);
    color: var(--text-2);
  }
  .icon-btn.on {
    color: var(--accent);
  }
  .mb {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stats {
    display: flex;
    align-items: center;
    gap: 10px;
    white-space: nowrap;
  }
  .add {
    color: var(--diff-add);
    font-family: var(--mono);
  }
  .del {
    color: var(--diff-del);
    font-family: var(--mono);
  }
  .viewed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
  .meter {
    width: 48px;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-active);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    background: var(--diff-add);
    transition: width var(--ease);
  }
  .spacer {
    flex: 1;
  }
  .low {
    color: var(--text-2);
  }
  .legend {
    position: relative;
  }
  .legend summary {
    list-style: none;
    cursor: pointer;
    color: var(--diff-hunk);
  }
  .legend summary::-webkit-details-marker {
    display: none;
  }
  .lpop {
    position: absolute;
    top: 24px;
    left: 0;
    z-index: 20;
    width: 360px;
    margin: 0;
    padding: 8px 10px;
    list-style: none;
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow);
    white-space: normal;
  }
  .lpop li {
    display: grid;
    grid-template-columns: 10px 84px 1fr;
    align-items: baseline;
    gap: 8px;
    padding: 3px 0;
    font-size: 11.5px;
  }
  .lpop b {
    font-weight: 600;
    color: var(--text);
  }
  .phone .stats {
    flex-wrap: wrap;
    row-gap: 4px;
  }
  .phone .legend[open] {
    flex-basis: 100%;
  }
  .phone .lpop {
    position: static;
    width: auto;
    margin-top: 6px;
  }
  .sw {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    align-self: center;
  }
  .upnext {
    flex: 0 1 auto;
    min-width: 150px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    max-width: 280px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-2);
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
  }
  .upnext .mono {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stitle {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .sort {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    margin-right: -6px;
    border-radius: 4px;
    color: var(--text-3);
  }
  .sort:hover {
    background: var(--surface-hover);
  }
  .sort.on {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 8px;
    border-radius: var(--radius);
    color: var(--text-2);
    font-size: 11.5px;
  }
  .live:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .live .dot {
    background: var(--st-input);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--st-input) 18%, transparent);
  }
  .live.paused .dot {
    background: var(--text-3);
    box-shadow: none;
  }
  .refresh {
    position: relative;
  }
  .refresh.stale::after {
    content: '';
    position: absolute;
    top: 3px;
    right: 3px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .main {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .side {
    width: 240px;
    flex: none;
    overflow: auto;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .stitle {
    padding: 12px 14px 2px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-3);
    text-transform: uppercase;
  }
  .files {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 14px 16px 24px;
    position: relative;
  }
  .note {
    margin: 0 0 10px;
    font-size: 12px;
  }
  .state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px 0;
  }
  .err {
    color: var(--st-failed);
  }
  .missing {
    flex-direction: column;
    gap: 6px;
    text-align: center;
    color: var(--text-2);
    padding: 48px 24px;
  }
  .missing strong {
    color: var(--text);
    font-size: 14px;
  }
  .missing .hint {
    max-width: 420px;
    font-size: 12.5px;
  }
  .missing .path {
    font-size: 12px;
    word-break: break-all;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11.5px;
  }
  .ro {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 10px;
    font-size: 12px;
  }
  .empty {
    max-width: 760px;
    margin: 0 auto;
    padding: 24px 0 0;
  }
  .empty > p {
    text-align: center;
    margin: 0 0 18px;
  }
  .others {
    margin-top: 6px;
  }
  .others .stitle {
    padding: 4px 0 6px;
  }
  .wt {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 12px;
    margin-bottom: 6px;
    text-align: left;
  }
  .wt:hover {
    border-color: var(--accent);
  }
  .wt .name {
    color: var(--text);
    font-weight: 600;
  }
  .phone .head {
    height: auto;
    min-height: 48px;
    flex-wrap: wrap;
    row-gap: 4px;
    padding: 6px 8px 6px 12px;
  }
  .phone .ref {
    flex: 1 1 100%;
  }
  .hide {
    display: none;
  }
  .phone .head .icon-btn {
    min-width: 44px;
    min-height: 44px;
  }
  .picker {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .picker select {
    width: 100%;
    height: 44px;
    font-size: 14px;
  }
  .tap {
    font-size: 12px;
  }
  .phone .files {
    padding: 0 0 16px;
  }
  .phone .files > :global(.commits),
  .phone .ro,
  .phone .empty {
    margin-left: 12px;
    margin-right: 12px;
  }
  .phone .files > :global(.commits) {
    margin-top: 10px;
  }
  .lgroup {
    gap: 5px;
    justify-content: flex-start;
    margin-top: 8px;
    text-transform: none;
    letter-spacing: 0;
  }
  .lgroup span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .linked {
    margin-top: 18px;
  }
  .lhead {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 8px 10px;
    margin-bottom: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    color: var(--text-2);
    font-size: 12px;
    text-align: left;
    white-space: nowrap;
  }
  .lhead:hover {
    border-color: var(--accent);
  }
  .lname {
    color: var(--text);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .lbranch {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .lhead .chev {
    display: inline-flex;
    transition: transform var(--ease);
  }
  .lhead .chev.open {
    transform: rotate(90deg);
  }
  .phone .linked {
    margin: 14px 0 0;
  }
  .phone .lhead {
    border-radius: 0;
    border-left: 0;
    border-right: 0;
    min-height: 44px;
  }
  .embedded .side {
    width: 210px;
  }
</style>
