<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import RotateCw from '@lucide/svelte/icons/rotate-cw';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import AppWindow from '@lucide/svelte/icons/app-window';
  import PanelRightClose from '@lucide/svelte/icons/panel-right-close';
  import Globe from '@lucide/svelte/icons/globe';
  import Loader from '@lucide/svelte/icons/loader-circle';
  import X from '@lucide/svelte/icons/x';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { isTauri } from '../../daemon/client';
  import { boundsOf, nativeBrowser, normalizeUrl } from '../../browser';
  import { pages } from '../../session/session-pages.svelte';
  import type { Page } from '../../session/session-pages';
  import { openInFocused, update } from '../../workspace/workspace.svelte';
  import { newTabKey } from '../../tabs/tabs';

  let { sessionKey, phone = false }: { sessionKey: string; phone?: boolean } = $props();

  const native = isTauri() && !app.mobile;
  const web = $derived(pages.byKey[sessionKey]);
  const page = $derived(web?.pages.find((p) => p.id === web.active) ?? web?.pages[0]);
  const loading = $derived(!!page && pages.loading.has(page.id));
  let input = $state('');
  let root: HTMLDivElement | undefined = $state();
  let host: HTMLDivElement | undefined = $state();
  let shown: string | null = null;
  let dragging = $state(false);

  const err = (e: unknown) => app.notify(String((e as Error)?.message ?? e), 'error');

  $effect(() => {
    input = page?.url ?? '';
  });

  $effect(() => {
    const id = page?.id ?? null;
    const hidden = !!app.overlay;
    const el = host;
    if (!native || !el) return;
    untrack(() => {
      if (shown && (shown !== id || hidden)) {
        pages.hide(shown);
        shown = null;
      }
      if (id && !hidden && shown !== id) {
        shown = id;
        pages.show(id, boundsOf(el)).catch(err);
      }
    });
  });

  function place() {
    if (shown && host) pages.bounds(shown, boundsOf(host));
  }

  onMount(() => {
    if (!native || !host) return;
    const ro = new ResizeObserver(place);
    ro.observe(host);
    window.addEventListener('resize', place);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', place);
      if (shown) pages.hide(shown);
      shown = null;
    };
  });

  function label(p: Page): string {
    if (p.title) return p.title;
    try {
      const u = new URL(p.url);
      return u.pathname === '/' ? u.host : `${u.host}${u.pathname}`;
    } catch {
      return p.url;
    }
  }

  function go(raw: string) {
    const next = normalizeUrl(raw);
    if (next && page) pages.navigate(page.id, next).catch(err);
  }

  function external() {
    if (!page) return;
    if (native) nativeBrowser.external(page.url).catch(err);
    else window.open(page.url, '_blank', 'noopener');
  }

  function detach() {
    if (!page) return;
    const key = newTabKey('browser', page.url);
    pages.close(sessionKey, page.id);
    update((l) => openInFocused(l, key));
    app.selectedKey = key;
  }

  function closePage(e: Event, id: string) {
    e.stopPropagation();
    pages.close(sessionKey, id);
  }

  function down(e: PointerEvent) {
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    const box = root?.parentElement?.getBoundingClientRect();
    if (dragging && box?.width) pages.setRatio(sessionKey, (box.right - e.clientX) / box.width);
  }

  function key(e: KeyboardEvent) {
    const step = e.key === 'ArrowLeft' ? 0.05 : e.key === 'ArrowRight' ? -0.05 : 0;
    if (step && web) pages.setRatio(sessionKey, web.ratio + step);
  }
</script>

<div class="web" class:phone class:dragging bind:this={root} style:flex-basis={phone ? undefined : `${(web?.ratio ?? 0.5) * 100}%`}>
  {#if !phone}
    <button class="divider" aria-label={t('web.resize')} onpointerdown={down} onpointermove={move} onpointerup={() => (dragging = false)} onpointercancel={() => (dragging = false)} onkeydown={key}></button>
  {/if}
  <div class="col">
    <div class="pages" role="tablist" aria-label={t('web.pages')}>
      {#each web?.pages ?? [] as p (p.id)}
        <div
          class="pg"
          class:active={p.id === page?.id}
          role="tab"
          tabindex="0"
          aria-selected={p.id === page?.id}
          title={p.url}
          onclick={() => pages.select(sessionKey, p.id)}
          onkeydown={(e) => e.key === 'Enter' && pages.select(sessionKey, p.id)}
          onauxclick={(e) => e.button === 1 && closePage(e, p.id)}
        >
          {#if pages.loading.has(p.id)}<span class="spin"><Loader size={12} /></span>{:else}<Globe size={12} />{/if}
          <span class="name">{label(p)}</span>
          <button class="x" aria-label={t('common.close')} onclick={(e) => closePage(e, p.id)}><X size={12} /></button>
        </div>
      {/each}
      <span class="grow"></span>
      <button class="icon-btn" title={t('web.hide')} aria-label={t('web.hide')} onclick={() => pages.setOpen(sessionKey, false)}><PanelRightClose size={14} /></button>
    </div>
    <div class="bar">
      {#if native}
        <button class="icon-btn" aria-label={t('browser.back')} title={t('browser.back')} onclick={() => page && pages.step(page.id, -1).catch(err)}><ArrowLeft size={15} /></button>
        <button class="icon-btn" aria-label={t('browser.forward')} title={t('browser.forward')} onclick={() => page && pages.step(page.id, 1).catch(err)}><ArrowRight size={15} /></button>
        <button class="icon-btn" aria-label={t('browser.reload')} title={t('browser.reload')} onclick={() => page && pages.reload(page.id).catch(err)}>
          {#if loading}<span class="spin"><Loader size={15} /></span>{:else}<RotateCw size={14} />{/if}
        </button>
      {/if}
      <form onsubmit={(e) => (e.preventDefault(), go(input))}>
        <input bind:value={input} placeholder={t('browser.url')} spellcheck="false" autocomplete="off" aria-label={t('browser.url')} onfocus={(e) => e.currentTarget.select()} />
      </form>
      <button class="icon-btn" aria-label={t('browser.external')} title={t('browser.external')} onclick={external} disabled={!page}><ExternalLink size={14} /></button>
      <button class="icon-btn" aria-label={t('web.detach')} title={t('web.detach')} onclick={detach} disabled={!page}><AppWindow size={14} /></button>
    </div>
    <div class="view" bind:this={host}>
      {#if page && !native}
        {#key page.id}
          <iframe src={page.url} title={page.url} sandbox="allow-scripts allow-same-origin allow-forms allow-popups"></iframe>
        {/key}
      {/if}
    </div>
  </div>
</div>

<style>
  .web {
    flex: 0 0 auto;
    display: flex;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
  }
  .web.phone {
    flex: 1;
  }
  .dragging {
    user-select: none;
    -webkit-user-select: none;
  }
  .divider {
    flex: none;
    width: 1px;
    padding: 0;
    border: 0;
    background: var(--border);
    cursor: col-resize;
    position: relative;
    transition: background var(--ease);
  }
  .divider::after {
    content: '';
    position: absolute;
    inset: 0 -3px;
  }
  .divider:hover,
  .dragging .divider {
    background: var(--accent);
  }
  .col {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .pages {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 32px;
    padding: 0 4px 0 6px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .pg {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 180px;
    height: 24px;
    padding: 0 4px 0 8px;
    border-radius: var(--radius);
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
    flex: none;
  }
  .pg:hover {
    background: var(--surface);
  }
  .pg.active {
    background: var(--surface);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .x {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .x:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 38px;
    padding: 0 6px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .spin {
    display: grid;
  }
  form {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    margin: 0 4px;
    padding: 0 10px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    transition: border-color var(--ease);
  }
  form:focus-within {
    border-color: var(--accent);
  }
  form input {
    flex: 1;
    min-width: 0;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--text);
    font-size: 12px;
  }
  .view {
    position: relative;
    flex: 1;
    min-height: 0;
    background: var(--bg);
  }
  iframe {
    width: 100%;
    height: 100%;
    border: 0;
    background: #fff;
  }
</style>
