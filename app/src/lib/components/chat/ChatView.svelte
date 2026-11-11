<script lang="ts">
  import { sentHistory } from '../../chat/history';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import { onMount, setContext, tick, untrack } from 'svelte';
  import { DaemonError } from '../../client';
  import { t } from '../../i18n.svelte';
  import { pausedDays, sessionState, stateDot, type SessionAction } from '../../session-state';
  import { runSessionAction } from '../../session-action';
  import SessionBar from '../session/SessionBar.svelte';
  import { app } from '../../state.svelte';
  import type { SidebarItem } from '../../grouping';
  import type { Term } from '../../types';
  import { applyEarlier, applyTail, applyUpdate, EMPTY_CHAT, needsFetch, type ChatState } from '../../chat/store';
  import { anchorOf, chats, type Anchor } from '../../chat/cache';
  import { hostKey } from '../../window';
  import { setMode } from '../../chat/viewmode.svelte';
  import { agentName, elapsedMs, formatElapsed, formatTokens, runningAgents } from '../../chat/agents';
  import { clock, tickWhile } from '../../chat/clock.svelte';
  import { gesture, isWebLink } from '../../links';
  import { Flights } from '../../chat/flight';
  import { closeConfirm } from '../../chat/close-confirm';
  import Entry from './Entry.svelte';
  import Composer from './Composer.svelte';
  import PromptCard from './PromptCard.svelte';
  import { isPickerCommand } from '../../chat/prompt';

  let { item, phone }: { item: SidebarItem; phone: boolean } = $props();

  const STEP = 60;
  const lineId = $derived(item.lineId!);
  const line = $derived(app.data.lines.find((l) => l.line.id === lineId));
  const agent = $derived(line?.line.agent ?? item.kind);
  const local = $derived(line?.local);
  const view = $derived(line ? sessionState(line, item.termId) : null);
  const alive = $derived(!!view && view.state !== 'paused' && view.state !== 'closed');
  const days = $derived(view?.state === 'paused' ? pausedDays(view.since, clock.now) : null);
  const waiting = $derived(!!local?.needsInput);

  let chat = $state.raw<ChatState>(EMPTY_CHAT);
  let unsupported = $state<string | null>(null);
  let failed = $state<string | null>(null);
  let shown = $state(STEP);
  let following = $state(true);
  let unseen = $state(0);
  let scroller = $state<HTMLDivElement>();
  let anchor: Anchor | null = null;
  let anchorTimer: ReturnType<typeof setTimeout> | undefined;
  let sentinel = $state<HTMLButtonElement>();
  const loads = new Flights();
  let fetching = false;
  let growing = false;
  let again = false;
  let stale = false;
  let acting = $state<SessionAction | null>(null);
  let confirmClose = $state(false);
  const closeArm = closeConfirm((v) => (confirmClose = v));
  $effect(() => {
    void lineId;
    return () => closeArm.reset();
  });
  let probe = $state(0);

  const visibleEntries = $derived(chat.entries.slice(Math.max(0, chat.entries.length - shown)));
  const hiddenLocal = $derived(chat.entries.length - visibleEntries.length);
  const model = $derived(chat.model);
  const running = $derived(runningAgents(chat.entries));

  setContext('chat-line', () => lineId);

  $effect(() => {
    if (running.length) return tickWhile();
  });

  async function reveal(id: string) {
    const find = () => scroller?.querySelector<HTMLElement>(`#agent-${CSS.escape(id)}`);
    if (!find()) {
      shown = chat.entries.length;
      await tick();
    }
    find()?.dispatchEvent(new CustomEvent('reveal'));
  }

  function visible(): boolean {
    return typeof document === 'undefined' || document.visibilityState === 'visible';
  }

  function message(e: unknown): string {
    const body = (e as DaemonError)?.body as { predicates?: Record<string, { ok: boolean; detail?: string }>; error?: string } | undefined;
    const failedPred = body?.predicates && Object.values(body.predicates).find((p) => !p.ok)?.detail;
    return failedPred ?? body?.error ?? String((e as Error)?.message ?? e);
  }

  // One cold load per line: watcher events and quick back-and-forth switches must not start parallel full parses.
  function load() {
    const id = lineId;
    return loads.run(id, () => loadTail(id));
  }

  async function loadTail(id: string) {
    try {
      const page = await app.api.chat(id, { limit: STEP * 2 });
      if (id !== lineId) {
        chats.put(cacheKey(id), { state: applyTail(EMPTY_CHAT, page), anchor: null, following: true, shown: STEP });
        return;
      }
      unsupported = null;
      failed = null;
      chat = applyTail(EMPTY_CHAT, page);
      shown = STEP;
      following = true;
      unseen = 0;
      await stick(true);
    } catch (e) {
      if (id !== lineId) return;
      if (e instanceof DaemonError && e.status === 409) unsupported = e.message;
      else failed = String((e as Error)?.message ?? e);
    }
  }

  async function update() {
    if (!chat.loaded || (chat.cut && following)) return void load();
    if (fetching) return void (again = true);
    fetching = true;
    try {
      do {
        again = false;
        if (!chat.loaded) break;
        const id = lineId;
        const before = chat.entries.length;
        const page = await app.api.chat(id, { after: chat.cursor });
        if (id !== lineId) continue;
        chat = applyUpdate(chat, page, undefined, following);
        const added = Math.max(0, chat.entries.length - before);
        if (following) await stick(false);
        else unseen += added;
      } while (again);
    } catch {
      stale = true;
    } finally {
      fetching = false;
    }
  }

  async function earlier() {
    if (growing) return;
    growing = true;
    try {
      await older();
    } finally {
      growing = false;
    }
  }

  async function older() {
    if (hiddenLocal > 0) return grow(() => (shown += STEP));
    const first = chat.entries[0]?.seq;
    if (first === undefined || !chat.more) return;
    const id = lineId;
    try {
      const page = await app.api.chat(id, { before: first, limit: 200 });
      if (id !== lineId) return;
      await grow(() => {
        const n = chat.entries.length;
        chat = applyEarlier(chat, page);
        shown += chat.entries.length - n;
      });
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async function grow(fn: () => void) {
    const el = scroller;
    const h = el?.scrollHeight ?? 0;
    fn();
    await tick();
    if (el) el.scrollTop += el.scrollHeight - h;
  }

  async function stick(force: boolean) {
    await tick();
    const el = scroller;
    if (!el || (!force && !following)) return;
    el.scrollTop = el.scrollHeight;
    unseen = 0;
  }

  const webLink = (e: MouseEvent) => {
    const a = (e.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
    return a && isWebLink(a.href) ? a.href : null;
  };

  function onclick(e: MouseEvent) {
    const href = webLink(e);
    if (e.defaultPrevented || e.button !== 0 || !href) return;
    e.preventDefault();
    app.openUrl(href, { key: item.key }, gesture(e));
  }

  function oncontextmenu(e: MouseEvent) {
    const href = webLink(e);
    if (!href) return;
    e.preventDefault();
    app.linkMenu(e, href, { key: item.key });
  }

  function onscroll() {
    const el = scroller;
    if (!el) return;
    const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    following = bottom;
    clearTimeout(anchorTimer);
    anchorTimer = bottom ? undefined : setTimeout(capture, 150);
    if (bottom) {
      unseen = 0;
      if (chat.cut) return void load();
      if (shown > STEP * 2) shown = STEP * 2;
    }
  }

  $effect(() => {
    const el = scroller;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => {
      if (following) el.scrollTop = el.scrollHeight;
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  function jump() {
    following = true;
    if (chat.cut) return void load();
    void stick(true);
  }

  const cacheKey = (id: string) => `${hostKey(app.conn.target)}|${id}`;
  let cachedAs: string | null = null;

  function capture() {
    const el = scroller;
    if (el?.isConnected) anchor = following ? null : anchorOf(el);
  }

  function remember() {
    clearTimeout(anchorTimer);
    capture();
    // A cut window lacks the newest entries: coming back must load a fresh tail, not show it.
    if (cachedAs && chat.cut) chats.drop(cachedAs);
    else if (cachedAs && chat.loaded && !unsupported && !failed) chats.put(cachedAs, { state: chat, anchor: following ? null : anchor, following, shown });
    anchor = null;
  }

  async function restore(at: Anchor | null) {
    await tick();
    const el = scroller;
    if (!el) return;
    const target = at && el.querySelector<HTMLElement>(`[data-id="${CSS.escape(at.id)}"]`);
    if (following || !at || !target) {
      following = true;
      el.scrollTop = el.scrollHeight;
      return;
    }
    if (document.visibilityState !== 'hidden') for (let i = 0; i < 2; i++) await new Promise((r) => requestAnimationFrame(r));
    if (!target.isConnected) return;
    el.scrollTop += target.getBoundingClientRect().top - el.getBoundingClientRect().top - at.offset;
    anchor = at;
  }

  $effect(() => {
    const id = lineId;
    untrack(() => {
      remember();
      cachedAs = cacheKey(id);
      unsupported = null;
      failed = null;
      unseen = 0;
      const hit = chats.get(cachedAs);
      if (!hit) {
        chat = EMPTY_CHAT;
        return void load();
      }
      chat = hit.state;
      shown = hit.shown;
      following = hit.following;
      void restore(hit.anchor);
      void update();
    });
  });

  // Scrolling near the top loads the previous window; the observer only fires on crossings, so no idle work.
  $effect(() => {
    const el = sentinel;
    const root = scroller;
    if (!el || !root || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver((es) => {
      if (es.some((e) => e.isIntersecting) && !following && chat.loaded) void earlier();
    }, { root, rootMargin: '400px 0px 0px 0px' });
    io.observe(el);
    return () => io.disconnect();
  });

  let phase = app.conn.phase;
  $effect(() => {
    const p = app.conn.phase;
    const was = phase;
    phase = p;
    if (p === 'connected' && was !== 'connected') untrack(() => void update());
  });

  onMount(() => {
    const off = app.onChat((d) => {
      if (d.lineId !== lineId || !needsFetch(chat, d.cursor)) return;
      if (visible()) void update();
      else stale = true;
    });
    const onVisibility = () => {
      if (visible() && stale) {
        stale = false;
        void update();
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      remember();
      clearTimeout(anchorTimer);
      off();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  });

  function toTerminal() {
    setMode(item, 'terminal');
  }

  async function act(kind: SessionAction) {
    if (kind === 'close' && !confirmClose) {
      closeArm.arm();
      return;
    }
    closeArm.reset();
    acting = kind;
    try {
      const r = await runSessionAction(app.api, lineId, kind);
      if (r.dropChat) chats.drop(cacheKey(lineId));
      const term = r.term as Term | undefined;
      if (term?.id) app.upsertTerm(term);
      if (r.notice) app.notify(t(r.notice));
    } catch (e) {
      app.notify(t('chat.actionFailed', { reason: message(e) }), 'error');
    } finally {
      acting = null;
    }
  }
</script>

<div class="chat" class:phone>
  <header class="head">
    <span class="state {stateDot(view)}"></span>
    <span class="status">{view ? t(view.label) : ''}{#if view?.sub}<span class="faint"> · {t(view.sub.key, 'n' in view.sub ? { n: view.sub.n } : undefined)}</span>{/if}{#if days !== null}<span class="faint"> · {t('session.pausedFor', { n: days })}</span>{/if}</span>
    {#if model}<span class="model faint">{model}</span>{/if}
    <span class="grow"></span>
    {#if view}<SessionBar {view} {acting} {confirmClose} onact={act} />{/if}
  </header>

  {#if unsupported}
    <div class="empty">
      <p class="muted">{t('chat.unsupported')}</p>
      <p class="faint small">{unsupported}</p>
      <button class="btn primary" onclick={toTerminal}><SquareTerminal size={13} />{t('chat.openTerminal')}</button>
    </div>
  {:else}
    <div class="list" bind:this={scroller} {onscroll} {onclick} {oncontextmenu} role="presentation">
      {#if hiddenLocal > 0 || chat.more}
        <button class="earlier" bind:this={sentinel} onclick={earlier}>{t('chat.loadEarlier')}</button>
      {/if}
      {#if view?.starting}
        <div class="starting">
          <p class="muted">{t('chat.starting')}</p>
          <button class="btn primary" onclick={toTerminal}><SquareTerminal size={13} />{t('chat.openTerminal')}</button>
        </div>
      {/if}
      {#if failed}
        <p class="note error">{failed}</p>
      {:else if !chat.loaded}
        <p class="note faint">{t('chat.loading')}</p>
      {:else if !chat.entries.length}
        <p class="note faint">{t('chat.empty')}</p>
      {/if}
      {#each visibleEntries as e (e.id)}
        <div class="item {e.kind}" data-id={e.id}><Entry entry={e} showModel={!phone} {lineId} /></div>
      {/each}
    </div>
    {#if !following && unseen > 0}
      <button class="jump" onclick={jump}><ArrowDown size={13} />{t('chat.newMessages', { n: unseen })}</button>
    {:else if !following}
      <button class="jump round" aria-label={t('chat.toBottom')} onclick={jump}><ArrowDown size={14} /></button>
    {/if}
    {#if running.length}
      <div class="agents" aria-label={t('chat.agents')}>
        <span class="lbl faint">{t('chat.agents')}</span>
        {#each running as r (r.id)}
          <button class="ag" title={r.agent.description} onclick={() => reveal(r.id)}>
            <span class="dot"></span>
            <span class="nm">{agentName(r.agent)}</span>
            <span class="ds">{r.agent.description}</span>
            <span class="tm faint">{formatElapsed(elapsedMs(r.agent, clock.now))}{r.agent.tokens ? ` · ↓ ${formatTokens(r.agent.tokens)}` : ''}</span>
          </button>
        {/each}
      </div>
    {/if}
    <PromptCard {lineId} {agent} {waiting} {probe} onterminal={toTerminal} />
    {#if alive}
      <Composer {lineId} {agent} {local} {phone} history={sentHistory(chat.entries)} onsent={(text) => { if (isPickerCommand(text)) probe++; }} />
    {/if}
  {/if}
</div>

<style>
  .chat {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 2px 8px 2px 12px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }
  .state {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--state-closed);
  }
  .state.input {
    background: var(--state-attention);
  }
  .state.busy {
    background: var(--state-running);
  }
  .state.paused {
    background: var(--state-paused);
  }
  .state.ended {
    background: transparent;
    box-shadow: inset 0 0 0 1px var(--state-closed);
  }
  .state.idle {
    background: var(--state-open);
  }
  .status {
    color: var(--text-2);
    white-space: nowrap;
  }
  .model {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 11px;
  }
  .grow {
    flex: 1;
  }
  .phone .head :global(.icon-btn) {
    min-width: 40px;
    min-height: 40px;
    justify-content: center;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    user-select: text;
    -webkit-user-select: text;
    padding: 12px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .phone .list {
    padding: 10px 10px 12px;
  }
  .list :global(button) {
    user-select: none;
    -webkit-user-select: none;
  }
  .item {
    content-visibility: auto;
    contain-intrinsic-size: auto 48px;
    max-width: 920px;
    width: 100%;
    margin: 0 auto;
  }
  .item.tool,
  .item.todos {
    contain-intrinsic-size: auto 32px;
  }
  .earlier {
    align-self: center;
    min-height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
  }
  .starting {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    margin: 16px auto 0;
    max-width: 420px;
    text-align: center;
    font-size: 13px;
  }
  .note {
    margin: 24px auto;
    font-size: 13px;
  }
  .error {
    color: var(--st-failed);
  }
  .jump {
    position: absolute;
    left: 50%;
    bottom: 96px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
    font-size: 12px;
    cursor: pointer;
    z-index: 4;
  }
  .jump.round {
    padding: 0;
    width: 32px;
    justify-content: center;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
  .small {
    font-size: 12px;
  }
  .agents {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-top: 1px solid var(--border);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .lbl {
    flex-shrink: 0;
    font-size: 11px;
  }
  .ag {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    max-width: 360px;
    min-height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 11.5px;
    cursor: pointer;
  }
  .ag:hover {
    background: var(--surface-hover);
  }
  .dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--st-working);
  }
  .nm {
    flex-shrink: 0;
    color: var(--text);
    font-weight: 600;
  }
  .ds {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tm {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }
  .phone .agents {
    padding: 6px 10px;
  }
  .phone .ag {
    max-width: 280px;
    min-height: 32px;
  }
</style>
