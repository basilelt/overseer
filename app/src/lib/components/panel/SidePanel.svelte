<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { chipLabel } from '../../profiles/profiles';
  import { app } from '../../state.svelte';
  import type { Diff, SessionCost as Cost } from '../../types';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { agentName, sessionsAvailable } from '../../agents';
  import { preload } from '../../idle';
  const diffView = preload(() => import('./DiffView.svelte'));
  import EditedFiles from './EditedFiles.svelte';
  import PanelActions from './PanelActions.svelte';
  import Section from './Section.svelte';
  const todosView = preload(() => import('./Todos.svelte'));
  const workLogView = preload(() => import('./WorkLog.svelte'));
  const gitView = () => import('../../git/PanelGit.svelte');
  import SessionCost from './SessionCost.svelte';
  import { formatCost } from '../../usage/format';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import { hint } from '../../commands/keyboard';
  import { openReview, openReviewBeside } from '../../review/open';
  import { inRepo } from '../../review/repo';
  import Maximize2 from '@lucide/svelte/icons/maximize-2';
  import { reviewApi } from '../../review/api';
  import { changed, changedLabel, changedWorktrees, counts, folder } from '../../review/scope';
  import type { Related } from '../../review/types';
  import { DaemonError } from '../../daemon/client';
  import { age } from '../../time';
  import { saveSettings, settings } from '../../settings/settings.svelte';
  import { moveSection, normalizeOrder, type PanelSection } from '../../panel/panelorder';
  import { diffs, relateds } from '../../panel/panel-cache';
  import { loadPanel } from '../../panel/panel-load';
  import { hostKey } from '../../window';
  import { STATE_COLOR } from '../../workspace/ws-color';

  const EFFORTS = new Set(['low', 'medium', 'high', 'xhigh', 'max']);
  const item = $derived(app.selected);
  const lineId = $derived(item?.lineId ?? null);
  const line = $derived(lineId ? app.data.lines.find((l) => l.line.id === lineId) : undefined);
  const act = $derived(lineId ? app.data.activity[lineId] : undefined);
  const sessions = $derived(line ? sessionsAvailable(app.agents, line.line.agent) : true);
  const chip = $derived(item?.termId ? app.data.terms.find((x) => x.id === item.termId)?.profile : undefined);
  const repo = $derived(inRepo(line?.line));
  const todosDone = $derived(act?.todos.filter((x) => x.state === 'completed').length ?? 0);

  let diff = $state<Diff | null>(null);
  let related = $state.raw<Related | null>(null);
  let missing = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let relTimer: ReturnType<typeof setTimeout> | undefined;

  const current = $derived(related?.worktrees.find((w) => w.current) ?? null);
  const changedList = $derived(changedWorktrees(related));

  const cacheKey = (id: string) => `${hostKey(app.conn.target)}|${id}`;

  const showing = (key: string) => !!lineId && key === cacheKey(lineId);

  async function loadDiff(id: string) {
    if (!repo) return;
    const key = cacheKey(id);
    const d = await app.api.diff(id).catch(() => null);
    if (d) diffs.set(key, d);
    if (showing(key)) diff = d;
  }

  async function loadRelated(id: string) {
    if (!app.client || !repo) return;
    const key = cacheKey(id);
    try {
      const r = await reviewApi(app.client).related({ lineId: id });
      relateds.set(key, r);
      if (!showing(key)) return;
      related = r;
      missing = false;
    } catch (e) {
      relateds.delete(key);
      if (!showing(key)) return;
      related = null;
      missing = e instanceof DaemonError && e.status === 410;
    }
  }

  function relatedSoon(id: string, ms: number) {
    clearTimeout(relTimer);
    relTimer = setTimeout(() => loadRelated(id), ms);
  }

  $effect(() => {
    const id = lineId;
    const inGit = repo;
    const scm = app.panelGit;
    missing = false;
    if (!id || !app.client || scm) {
      clearTimeout(timer);
      clearTimeout(relTimer);
      diff = null;
      related = null;
      return;
    }
    untrack(() => {
      diff = inGit ? (diffs.get(cacheKey(id)) ?? null) : null;
      related = inGit ? (relateds.get(cacheKey(id)) ?? null) : null;
      loadPanel(id, inGit, { activity: (x) => app.loadActivity(x), diff: loadDiff, related: loadRelated });
    });
  });

  onMount(() => {
    const off = app.onChange((_p, _n, e) => {
      if (!lineId || app.panelGit) return;
      const id = lineId;
      if (e.type === 'line.activity' && e.data?.lineId === id) {
        clearTimeout(timer);
        timer = setTimeout(() => loadDiff(id), 1500);
        relatedSoon(id, 1500);
      } else if (e.type === 'review.changed' || e.type === 'worktree.created' || e.type === 'worktree.removed') {
        relatedSoon(id, 2000);
      }
    });
    return () => {
      off();
      clearTimeout(timer);
      clearTimeout(relTimer);
      cancelDrag();
    };
  });

  const summary = (w: { files: number; ahead: number }) => counts({ files: w.files, ahead: w.ahead });

  const sectionOrder = $derived(normalizeOrder(settings.panelOrder));
  const orderOf = (id: PanelSection) => sectionOrder.indexOf(id) + 1;
  const costLine = (c: Cost) =>
    [formatCost(c.cost, c.partial), c.model, c.effort ? t('profile.effortOf', { effort: (EFFORTS.has(c.effort) ? t(`effort.${c.effort}` as Key) : c.effort).toLowerCase() }) : ''].filter(Boolean).join(' · ');
  let dragging: { id: string; y: number; moved: boolean; el: HTMLElement } | null = null;
  let swallow = false;

  function dragDown(e: PointerEvent) {
    const head = (e.target as HTMLElement).closest('.head');
    const sec = head?.closest<HTMLElement>('[data-sec]');
    if (!sec || e.button !== 0 || e.pointerType !== 'mouse') return;
    dragging = { id: sec.dataset.sec!, y: e.clientY, moved: false, el: sec };
    window.addEventListener('pointermove', dragMove);
    window.addEventListener('pointerup', dragUp);
    window.addEventListener('pointercancel', dragCancel);
    window.addEventListener('blur', dragCancel);
  }

  function cancelDrag() {
    window.removeEventListener('pointermove', dragMove);
    window.removeEventListener('pointerup', dragUp);
    window.removeEventListener('pointercancel', dragCancel);
    window.removeEventListener('blur', dragCancel);
    const d = dragging;
    dragging = null;
    if (!d) return;
    d.el.classList.remove('dragging');
    document.body.style.cursor = '';
  }

  function dragCancel() {
    cancelDrag();
  }

  function dragMove(e: PointerEvent) {
    if (!dragging) return;
    if (!dragging.moved && Math.abs(e.clientY - dragging.y) < 6) return;
    dragging.moved = true;
    dragging.el.classList.add('dragging');
    document.body.style.cursor = 'grabbing';
  }

  function dragUp(e: PointerEvent) {
    const d = dragging;
    cancelDrag();
    if (!d) return;
    if (!d.moved) return;
    swallow = true;
    const target = [...(d.el.parentElement?.querySelectorAll<HTMLElement>('[data-sec]') ?? [])].find((el) => {
      const r = el.getBoundingClientRect();
      return e.clientY >= r.top && e.clientY <= r.bottom;
    });
    if (!target || target === d.el) return;
    const r = target.getBoundingClientRect();
    saveSettings({ panelOrder: moveSection(settings.panelOrder, d.id, target.dataset.sec!, e.clientY > r.top + r.height / 2) });
  }

  function swallowClick(e: MouseEvent) {
    if (!swallow) return;
    swallow = false;
    e.stopPropagation();
    e.preventDefault();
  }
</script>

<aside class="panel" aria-label={t('panel.title')} data-tour="panel">
  <div class="tabs" role="tablist" aria-label={t('panel.title')}>
    <button role="tab" aria-selected={!app.panelGit} class:on={!app.panelGit} onclick={() => app.setPanelGit(false)}>{t('panel.title')}</button>
    <button role="tab" title={`${t('git.scm')} (${hint('sourceControl')})`} aria-selected={app.panelGit} class:on={app.panelGit} data-tour="panel-git-tab" onclick={() => app.setPanelGit(true)}><GitBranch size={13} />{t('git.side.tab')}</button>
  </div>
  {#if app.panelGit}
    {#await gitView() then { default: PanelGit }}<PanelGit />{/await}
  {:else if !item || !line}
    <p class="empty faint">{item ? t('panel.noActivity') : t('panel.noSelection')}</p>
  {:else}
    <header>
      <div class="title">
        <span class="dot" style:background={STATE_COLOR[item.state]}></span>
        <h3>{item.renamed ? item.title : act?.title || item.title}</h3>
      </div>
      {#if act?.goal || item.goal}<p class="goal muted">{act?.goal || item.goal}</p>{/if}
      {#if act?.needsInput || line.local?.needsInput}
        <span class="badge">{t('panel.needsInput')}</span>
      {/if}
      <p class="meta faint">
        <span class="agent"><AgentGlyph agent={line.line.agent} size={12} />{agentName(app.agents, line.line.agent)}</span>
        <span class="mono">{line.line.branch ?? ''}{line.line.ticketKey ? ` · ${line.line.ticketKey}` : ''}</span>
      </p>
      {#if act?.cost}<p class="profile faint mono" data-tour="session-cost" title={t('cost.hint', { date: act.cost.pricesAsOf })}>{costLine(act.cost)}</p>{/if}
      {#if chip}<p class="profile faint" title={t('profile.label')}>{chipLabel(chip, (e) => t('profile.effortOf', { effort: t(`effort.${e}` as Key).toLowerCase() }))}</p>{/if}
    </header>
    <PanelActions {line} />
    <div class="scroll" role="presentation" onpointerdown={dragDown} onclickcapture={swallowClick}>
      {#if act}
        {#if act.lastAssistantText}
          <Section id="last" order={orderOf('last')} title={t('panel.lastMessage')}>
            <p class="msg">{act.lastAssistantText.replace(/`/g, '')}</p>
          </Section>
        {/if}
        {#if act.workLog.length}
          <Section id="log" order={orderOf('log')} title={t('panel.workLog')} meta={String(act.workLog.length)}>
            {#await workLogView() then { default: WorkLog }}<WorkLog steps={act.workLog} />{/await}
          </Section>
        {/if}
        {#if act.todos.length}
          <Section id="todos" order={orderOf('todos')} title={t('panel.todos')} meta={t('panel.todosProgress', { done: todosDone, total: act.todos.length })}>
            {#await todosView() then { default: Todos }}<Todos todos={act.todos} />{/await}
          </Section>
        {/if}
        {#if act.cost}
          <Section id="cost" order={orderOf('cost')} title={t('cost.title')} meta={formatCost(act.cost.cost, act.cost.partial)}>
            <SessionCost cost={act.cost} />
          </Section>
        {/if}
        {#if act.editedFiles.length}
          <Section id="files" order={orderOf('files')} title={t('panel.editedFiles')} meta={String(act.editedFiles.length)}>
            <EditedFiles files={act.editedFiles} />
          </Section>
        {/if}
      {:else if !sessions}
        <p class="empty faint">{t('panel.noSessions', { agent: agentName(app.agents, line.line.agent) })}</p>
      {:else}
        <p class="empty faint">{t('panel.noActivity')}</p>
      {/if}
      <Section id="diff" order={orderOf('diff')} title={t('panel.diff')} meta={!repo ? '' : current ? summary(current) : (diff?.stat.trim().split(',')[0] ?? '')}>
        {#if !repo}
          <p class="faint small">{t('panel.notRepo')}</p>
        {:else}
        <div class="review-btns">
          <button class="btn open-review" data-tour="review" onclick={() => lineId && openReviewBeside({ lineId })}><GitPullRequest size={13} />{t('review.open')}</button>
          <button class="icon-btn" title={t('review.asTab')} aria-label={t('review.asTab')} onclick={() => lineId && openReview({ lineId })}><Maximize2 size={13} /></button>
        </div>
        {#if missing}<p class="small missing">{t('review.missing')}</p>{/if}
        {#if changedList.length && !(changedList.length === 1 && changedList[0].current)}
          <div class="wts">
            <div class="sub faint">{changedLabel(changedList)}</div>
            {#each changedList as w (w.path)}
              <button class="wt" class:cur={w.current} onclick={() => (w.current && lineId ? openReview({ lineId }) : openReview({ worktree: w.path }))}>
                <span class="mono name">{w.branch || folder(w.path)}</span>
                <span class="faint">{summary(w)} · {age(w.at)}</span>
              </button>
            {/each}
          </div>
        {/if}
        {#if diff && diff.diff.trim()}
          {#if diff.truncated}<p class="faint small">{t('panel.diffTruncated')}</p>{/if}
          {#await diffView() then { default: DiffView }}<DiffView diff={diff.diff} />{/await}
        {:else if current && changed(current)}
          <p class="faint small">{summary(current)}</p>
        {:else}
          <p class="faint small">{t('panel.diffEmpty')}</p>
        {/if}
        {/if}
      </Section>
    </div>
  {/if}
</aside>

<style>
  .review-btns {
    display: flex;
    gap: 6px;
    margin-bottom: 8px;
  }
  .open-review {
    flex: 1;
    justify-content: center;
  }
  .missing {
    color: var(--st-failed);
    margin: 0 0 8px;
  }
  .wts {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }
  .wts .sub {
    font-size: 11.5px;
    margin-bottom: 2px;
  }
  .wt {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-size: 11.5px;
    text-align: left;
    min-width: 0;
  }
  .wt:hover {
    border-color: var(--accent);
  }
  .wt.cur {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .wt .name {
    color: var(--text);
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin: 10px 14px 0;
    padding: 2px;
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .tabs button {
    flex: 1 1 0;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 24px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    white-space: nowrap;
    transition:
      background var(--ease),
      color var(--ease);
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  :global(:root.phone) .tabs button {
    height: 40px;
    border-radius: 8px;
    font-size: 14px;
  }
  .panel {
    width: var(--panel-w, 340px);
    flex: none;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--bg-subtle);
    min-height: 0;
  }
  header {
    padding: 14px 14px 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .goal {
    margin: 4px 0 0;
    font-size: 12px;
  }
  .profile {
    margin: 4px 0 0;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 0;
    font-size: 11px;
    min-width: 0;
  }
  .meta .agent {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .meta .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    font-size: 12px;
    color: var(--st-input);
  }
  .badge::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--st-input);
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .scroll > :global(section) {
    flex: none;
  }
  .scroll :global(section.dragging) {
    opacity: 0.5;
  }
  .scroll :global(.head) {
    cursor: grab;
  }
  .msg {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
    white-space: pre-wrap;
    user-select: text;
    -webkit-user-select: text;
  }
  .empty {
    padding: 24px 14px;
    text-align: center;
  }
  .small {
    font-size: 12px;
    margin: 0 0 6px;
  }
</style>
