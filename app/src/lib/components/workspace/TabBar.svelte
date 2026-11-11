<script lang="ts" module>
  import { ensureSprite } from '../../icons/sprite';

  ensureSprite();
</script>

<script lang="ts">
  import { ust } from '../../usage/strings';
  import { gt } from '../../github/strings';
  import Columns2 from '@lucide/svelte/icons/columns-2';
  import Rows2 from '@lucide/svelte/icons/rows-2';
  import X from '@lucide/svelte/icons/x';
  import OctagonX from '@lucide/svelte/icons/octagon-x';
  import Plus from '@lucide/svelte/icons/plus';
  import Globe from '@lucide/svelte/icons/globe';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import Megaphone from '@lucide/svelte/icons/megaphone';
  import { st } from '../../system/strings';
  import { settings } from '../../settings.svelte';
  import { projectName } from '../../state.svelte';
  import { coarsePointer, focusSession, stripScroll, tabInfo, type TabInfo } from '../../tabs';
  import { broadcastTo, newSessionItems } from '../../wsactions';
  import { wsDropTarget } from '../../workspaces';
  import { t, type Key } from '../../i18n.svelte';
  import { app, tabLazy } from '../../state.svelte';
  import { whenIdle } from '../../idle';
  import { closePaneAt, closeTabAt, openInFocused, reorderTab, split, update, type Pane } from '../../workspace.svelte';
  import AgentIcon, { GROUP_COLOR } from '../sidebar/AgentIcon.svelte';
  import ReviewBadge from '../review/ReviewBadge.svelte';
  import { itemTarget, reviewTitle } from '../../review/open';
  import { liveOn } from '../../git/live.svelte';
  import { chatCapable } from '../../chat/viewmode';
  import ModeToggle from '../chat/ModeToggle.svelte';
  import { chipLabel, chipShort } from '../../profiles';
  import { pages } from '../../session-pages.svelte';

  let { pane, index, focused, canClose }: { pane: Pane; index: number; focused: boolean; canClose: boolean } = $props();

  const activeItem = $derived(pane.active ? app.items.find((i) => i.key === pane.active) : undefined);
  const activeWeb = $derived(activeItem ? pages.byKey[activeItem.key] : undefined);
  const tabs = $derived(pane.tabs.map((k) => tabInfo(k, app.items)).filter((x): x is TabInfo => !!x));
  const inWorkspace = $derived(app.sidebarMode === 'workspaces' && !app.workspaceId.startsWith('group:') && !!app.currentWorkspace);
  let plus: HTMLButtonElement | undefined = $state();
  let drag: { key: string; x: number; active: boolean } | null = null;
  let dropAt = $state<number | null>(null);
  let strip: HTMLDivElement | undefined = $state();

  function wheel(e: WheelEvent) {
    if (!strip || settings.wrapTabs || Math.abs(e.deltaX) > Math.abs(e.deltaY) || strip.scrollWidth <= strip.clientWidth) return;
    e.preventDefault();
    strip.scrollLeft += e.deltaY;
  }

  $effect(() => {
    const el = strip;
    if (!el) return;
    el.addEventListener('wheel', wheel, { passive: false });
    return () => el.removeEventListener('wheel', wheel);
  });

  $effect(() => {
    const key = pane.active;
    const el = strip;
    if (!el || !key) return;
    const frame = requestAnimationFrame(() => {
      if (el.scrollWidth <= el.clientWidth) return;
      const tab = el.querySelector<HTMLElement>(`[data-tab="${CSS.escape(key)}"]`);
      if (!tab) return;
      const box = el.getBoundingClientRect();
      const r = tab.getBoundingClientRect();
      const next = stripScroll({ left: box.left + el.clientLeft, width: el.clientWidth, scroll: el.scrollLeft }, r);
      if (next !== null) el.scrollLeft = next;
    });
    return () => cancelAnimationFrame(frame);
  });

  const chipOf = (tab: TabInfo) => (tab.item?.termId ? app.data.terms.find((x) => x.id === tab.item!.termId)?.profile : undefined);
  const effortLabel = (e: string) => t('profile.effortOf', { effort: t(`effort.${e}` as Key).toLowerCase() });

  function label(tab: TabInfo): string {
    if (tab.item) return tab.item.title;
    if (tab.kind === 'browser') {
      try {
        return tab.arg ? new URL(tab.arg).host || tab.arg : t('tabs.browser');
      } catch {
        return tab.arg || t('tabs.browser');
      }
    }
    if (tab.kind === 'review') return t('tabs.review', { title: reviewTitle(tab.key) ?? tab.arg });
    if (tab.kind === 'graph') return t('tabs.graphOf', { name: projectName(app.projects, tab.arg) });
    if (tab.kind === 'worktrees') return t('tabs.worktreesOf', { name: projectName(app.projects, tab.arg) });
    if (tab.kind === 'system') return st('title');
    if (tab.kind === 'usage') return ust('title');
    if (tab.kind === 'prs') return gt('title');
    return tab.key;
  }

  function openAdd(e?: MouseEvent) {
    const r = plus?.getBoundingClientRect();
    app.openMenu(e && e.clientX ? { clientX: e.clientX, clientY: e.clientY } : { clientX: r?.left ?? 0, clientY: (r?.bottom ?? 0) + 4 }, t('tabs.new'), newSessionItems());
  }

  $effect(() => {
    if (!focused) return;
    const fn = () => openAdd();
    window.addEventListener('sb:newtab', fn);
    return () => window.removeEventListener('sb:newtab', fn);
  });

  function tabDown(e: PointerEvent, key: string) {
    if (e.button !== 0) return;
    drag = { key, x: e.clientX, active: false };
  }

  function tabMove(e: PointerEvent) {
    if (!drag || !strip) return;
    if (!drag.active && Math.abs(e.clientX - drag.x) < 6) return;
    drag.active = true;
    const ws = app.canMoveToWorkspace(drag.key) ? wsDropTarget(document.elementFromPoint(e.clientX, e.clientY)) : null;
    app.tabDrop = ws && ws !== app.itemWs.get(drag.key) ? ws : null;
    if (app.tabDrop) {
      dropAt = null;
      return;
    }
    const els = [...strip.querySelectorAll<HTMLElement>('[data-tab]')];
    let at = els.length;
    for (let i = 0; i < els.length; i++) {
      const r = els[i].getBoundingClientRect();
      if (e.clientX < r.left + r.width / 2) {
        at = i;
        break;
      }
    }
    dropAt = at;
  }

  function tabUp() {
    const d = drag;
    if (!d) return;
    drag = null;
    const at = dropAt;
    dropAt = null;
    const ws = app.tabDrop;
    app.tabDrop = null;
    if (d.active && ws) {
      void app.moveToWorkspace(d.key, ws);
      suppress = true;
      setTimeout(() => (suppress = false), 0);
      return;
    }
    if (!d.active || at === null) return;
    const target = tabs[at]?.key;
    const to = target ? pane.tabs.indexOf(target) : pane.tabs.length;
    update((l) => reorderTab(l, index, d.key, to));
    suppress = true;
    setTimeout(() => (suppress = false), 0);
  }

  let suppress = false;

  // The rename input loads in the first idle moment, or on first use if sooner.
  let Extra = $state.raw<typeof import('./TabTitleExtra.svelte').default | null>(null);
  const loadExtra = () => (Extra ? Promise.resolve() : tabLazy().then((m) => void (Extra = m.TabTitleExtra)));
  $effect(() => whenIdle(() => void loadExtra().catch(() => {})));

  function click(e: MouseEvent, key: string) {
    if (suppress) return;
    if (e.metaKey || e.ctrlKey || e.shiftKey) {
      const next = new Set(app.multi);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      if (pane.active && !next.size) next.add(pane.active);
      app.multi = next;
      return;
    }
    app.multi = new Set();
    activate(key);
    if (e.detail > 0 && !coarsePointer() && tabInfo(key, app.items)?.item) {
      const pane = (e.currentTarget as HTMLElement | null)?.closest('.pane');
      requestAnimationFrame(() => focusSession(pane));
    }
  }

  function tabMenu(e: MouseEvent, tab: TabInfo) {
    e.preventDefault();
    void tabLazy().then((m) => app.openMenu(e, label(tab), m.tabMenu(app, e, index, tab.key, !!tab.item)));
  }
  const reviewable = $derived(!!pane.active && !!itemTarget(pane.active));

  const term = $derived(activeItem?.termId ? app.data.terms.find((x) => x.id === activeItem.termId) : undefined);
  const killable = $derived(!!term && !term.exited);

  let confirming = $state(false);
  let killing = $state(false);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    term?.id;
    confirming = false;
    clearTimeout(confirmTimer);
  });

  async function kill() {
    if (!term || !killable) return;
    if (!confirming) {
      confirming = true;
      confirmTimer = setTimeout(() => (confirming = false), 3000);
      return;
    }
    clearTimeout(confirmTimer);
    confirming = false;
    killing = true;
    try {
      await app.api.killTerm(term.id);
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      killing = false;
    }
  }

  function activate(key: string) {
    if (key.startsWith('line:') || key.startsWith('term:')) app.select(key);
    else {
      update((l) => openInFocused({ ...l, focused: index }, key));
      app.selectedKey = key;
    }
  }

  function close(e: MouseEvent, key: string) {
    e.stopPropagation();
    closeTabAt(index, key);
  }

  let editing = $state<{ key: string; initial: string; openedAt: number } | null>(null);

  function startEdit(e: MouseEvent, tab: TabInfo) {
    if (!(e.target as HTMLElement).closest('.name') || (!tab.item?.lineId && !tab.item?.termId)) return;
    e.stopPropagation();
    const edit = { key: tab.key, initial: tab.item.title, openedAt: performance.now() };
    if (Extra) editing = edit;
    else void loadExtra().then(() => (editing = edit));
  }
</script>

<svelte:window onpointermove={tabMove} onpointerup={tabUp} onpointercancel={tabUp} />

<div class="tabbar-box">
<div class="tabbar" class:focused class:wrap={settings.wrapTabs} class:dragging={dropAt !== null} role="tablist" data-tour="tabs">
  <div class="tabs" bind:this={strip}>
    {#each tabs as tab, i (tab.key)}
      {@const key = tab.key}
      <div
        class="tab"
        class:active={pane.active === key}
        class:multi={app.multi.has(key)}
        class:drop={dropAt === i}
        data-tab={key}
        role="tab"
        tabindex="0"
        title={tab.item?.status && tab.item.status !== label(tab) ? tab.item.status : label(tab)}
        aria-selected={pane.active === key}
        onclick={(e) => click(e, key)}
        onkeydown={(e) => e.key === 'Enter' && activate(key)}
        onauxclick={(e) => e.button === 1 && close(e, key)}
        oncontextmenu={(e) => tabMenu(e, tab)}
        onpointerdown={(e) => tabDown(e, key)}
        ondblclick={(e) => startEdit(e, tab)}
      >
        {#if tab.item}
          <AgentIcon kind={tab.item.kind} size={12} />
          <span class="dot" style:background={GROUP_COLOR[tab.item.group]}></span>
        {:else if tab.kind === 'browser'}
          <span class="kind"><svg class="i" width="12" height="12" aria-hidden="true"><use href="#sbi-globe" /></svg></span>
        {:else if tab.kind === 'graph'}
          <span class="kind"><svg class="i" width="12" height="12" aria-hidden="true"><use href="#sbi-git-branch" /></svg></span>
        {:else if tab.kind === 'worktrees'}
          <span class="kind"><svg class="i" width="12" height="12" aria-hidden="true"><use href="#sbi-folder-tree" /></svg></span>
        {:else}
          <span class="kind"><svg class="i" width="12" height="12" aria-hidden="true"><use href="#sbi-git-compare" /></svg></span>
        {/if}
        <span class="label">
          <span class="name">{label(tab)}</span>
          {#if Extra && editing?.key === key}
            <Extra tabKey={key} edit={editing} ondone={() => editing?.key === key && (editing = null)} />
          {/if}
        </span>
        {#if chipOf(tab)}{@const chip = chipOf(tab)}<span class="chip mono" title={chipLabel(chip, effortLabel)}>{chipShort(chip)}</span>{/if}
        <ReviewBadge tabKey={key} />
        <span class="slot"><span class="num faint">{i < 9 ? i + 1 : ''}</span><button class="x" aria-label={t('common.close')} onclick={(e) => close(e, key)}><svg class="i" width="12" height="12" aria-hidden="true"><use href="#sbi-x" /></svg></button></span>
      </div>
    {/each}
    <div class="end" class:drop={dropAt === tabs.length}></div>
  </div>
  <div class="actions">
    {#if activeItem && chatCapable(activeItem)}<ModeToggle item={activeItem} />{/if}
    {#if reviewable}
      <button class="icon-btn" class:on={liveOn()} title={t('git.button')} aria-label={t('git.button')} aria-pressed={liveOn()} data-tour="git-button" onclick={() => void import('../../git/live-actions').then((m) => m.toggleLiveReview(pane.active))}><GitBranch size={14} /></button>
    {/if}
    {#if activeWeb?.pages.length}
      <button class="icon-btn web" class:on={activeWeb.open} title={t('web.toggle')} aria-label={t('web.toggle')} aria-pressed={activeWeb.open} onclick={() => activeItem && pages.setOpen(activeItem.key, !activeWeb.open)}
        ><Globe size={14} />{#if activeWeb.pages.length > 1}<span>{activeWeb.pages.length}</span>{/if}</button
      >
    {/if}
    <button class="icon-btn" bind:this={plus} title={t('tabs.new')} aria-label={t('tabs.new')} aria-haspopup="menu" disabled={!app.connected} onclick={() => openAdd()}
      ><Plus size={14} /></button
    >
    {#if inWorkspace && app.currentWorkspace}
      <button
        class="icon-btn"
        title={t('broadcast.title')}
        aria-label={t('broadcast.title')}
        onclick={() => broadcastTo({ kind: 'workspace', id: app.currentWorkspace!.id })}><Megaphone size={14} /></button
      >
    {/if}
    {#if term}
      <button
        class="icon-btn kill"
        class:confirm={confirming}
        disabled={!killable || killing}
        title={!killable ? t('term.killExited') : confirming ? t('term.killConfirm') : t('term.kill')}
        aria-label={confirming ? t('term.killConfirm') : t('term.kill')}
        onclick={kill}
        onblur={() => (confirming = false)}
      >
        <OctagonX size={14} />{#if confirming}<span>{t('term.killConfirmShort')}</span>{/if}
      </button>
    {/if}
    <button class="icon-btn" title={t('term.splitRight')} aria-label={t('term.splitRight')} onclick={() => update((l) => split(l, 'horizontal'))}
      ><Columns2 size={14} /></button
    >
    <button class="icon-btn" title={t('term.splitDown')} aria-label={t('term.splitDown')} onclick={() => update((l) => split(l, 'vertical'))}
      ><Rows2 size={14} /></button
    >
    {#if canClose}
      <button class="icon-btn" title={t('term.closePane')} aria-label={t('term.closePane')} onclick={() => closePaneAt(index)}
        ><X size={14} /></button
      >
    {/if}
  </div>
</div>
</div>

<style>
  .actions .icon-btn.on {
    color: var(--accent);
  }
  .icon-btn.web {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    width: auto;
    padding: 0 6px;
    font-size: 11px;
  }
  .tabbar-box {
    container-type: inline-size;
    flex: none;
  }
  .tabbar {
    display: flex;
    align-items: center;
    height: 34px;
    flex: none;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
    padding-right: 4px;
  }
  .tabs {
    flex: 1;
    display: flex;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tabbar.wrap {
    height: auto;
    min-height: 34px;
    align-items: flex-start;
  }
  .wrap .tabs {
    flex-wrap: wrap;
    overflow: visible;
  }
  .wrap .tab {
    flex: 0 0 auto;
    border-bottom: 1px solid var(--border);
    margin-bottom: -1px;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 6px 0 10px;
    max-width: 180px;
    flex: none;
    border-right: 1px solid var(--border);
    color: var(--text-2);
    cursor: pointer;
    position: relative;
    transition: background var(--ease);
  }
  .tab:hover {
    background: var(--surface-hover);
  }
  .tab.active,
  .tab:has(:global(.name-input)) {
    max-width: none;
  }
  .tab.active {
    background: var(--bg);
    color: var(--text);
  }
  .focused .tab.active::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 2px;
    background: var(--accent);
  }
  .tab :global(.icon) {
    width: 18px;
    height: 18px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tab .label {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    min-width: 28px;
  }
  .tab:has(:global(.name-input)) {
    min-width: 200px;
  }
  .slot {
    display: grid;
    flex: none;
  }
  .slot > * {
    grid-area: 1 / 1;
  }
  /* While renaming, the input takes the title's place. */
  .label:has(:global(.name-input)) > .name {
    display: none;
  }
  .chip {
    flex: 0 6 auto;
    min-width: 0;
    max-width: 64px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
    color: var(--text-3);
  }
  .x {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: var(--text-3);
    opacity: 0;
  }
  .tab:hover .x,
  .tab.active .x {
    opacity: 1;
  }
  .x:hover {
    background: var(--surface-active);
    color: var(--text);
  }
  .tab.multi {
    background: var(--accent-soft);
  }
  .tab.drop,
  .end.drop {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .end {
    width: 4px;
    flex: none;
  }
  .kind {
    display: inline-grid;
    place-items: center;
    width: 18px;
    color: var(--text-2);
  }
  .num {
    place-self: center;
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }
  .tab:hover .num,
  .tab.active .num {
    visibility: hidden;
  }
  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 2px;
    margin-left: 6px;
    padding-left: 6px;
    border-left: 1px solid var(--border);
  }
  @container (max-width: 520px) {
    .tabbar {
      flex-wrap: wrap;
      height: auto;
      align-items: flex-start;
    }
    .tabs {
      flex: 1 0 100%;
    }
    .actions {
      margin-left: 0;
      padding-left: 0;
      border-left: 0;
      flex: 1 0 100%;
      flex-wrap: wrap;
      justify-content: flex-end;
      min-height: 34px;
      border-top: 1px solid var(--border);
    }
  }
  .kill:hover:not(:disabled),
  .kill.confirm {
    color: var(--st-failed);
  }
  .kill.confirm {
    display: inline-flex;
    align-items: center;
    width: auto;
    gap: 4px;
    padding: 0 8px;
    background: var(--diff-del-bg);
    font-size: 12px;
  }
  .kill:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
