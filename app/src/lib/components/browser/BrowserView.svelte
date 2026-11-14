<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import RotateCw from '@lucide/svelte/icons/rotate-cw';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Globe from '@lucide/svelte/icons/globe';
  import Loader from '@lucide/svelte/icons/loader-circle';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { isTauri } from '../../platform';
  import { boundsOf, nativeBrowser, normalizeUrl, type NavEvent } from '../../browser';

  let { initialUrl = null, onurl }: { initialUrl?: string | null; onurl?: (url: string) => void } = $props();

  const native = isTauri();
  const start = untrack(() => initialUrl) ?? app.browserUrl;
  let input = $state(start);
  let url = $state(normalizeUrl(start));
  let loading = $state(false);
  let opened = false;
  let host: HTMLDivElement | undefined = $state();
  let frame: HTMLIFrameElement | undefined = $state();
  let history: string[] = [];
  let pos = -1;

  const err = (e: unknown) => app.notify(String((e as Error)?.message ?? e), 'error');

  function onNav(e: NavEvent) {
    if (e.url) {
      url = e.url;
      input = e.url;
      app.browserUrl = e.url;
      onurl?.(e.url);
    }
    if (e.loading !== undefined) loading = e.loading;
  }

  function place() {
    if (!native || !host || !url) return;
    const b = boundsOf(host);
    if (!opened) {
      opened = true;
      loading = true;
      nativeBrowser.open(url, b, onNav).catch(err);
    } else nativeBrowser.bounds(b).catch(() => {});
  }

  function go(raw: string) {
    const next = normalizeUrl(raw);
    if (!next) return;
    url = next;
    input = next;
    app.browserUrl = next;
    onurl?.(next);
    if (native) {
      if (opened) nativeBrowser.navigate(next).catch(err);
      else queueMicrotask(place);
    } else {
      history = [...history.slice(0, pos + 1), next];
      pos = history.length - 1;
      loading = true;
    }
  }

  function step(delta: number) {
    if (native) return (delta < 0 ? nativeBrowser.back() : nativeBrowser.forward()).catch(err);
    const p = pos + delta;
    if (p < 0 || p >= history.length) return;
    pos = p;
    url = input = history[p];
    loading = true;
  }

  function reload() {
    if (native) return nativeBrowser.reload().catch(err);
    if (frame) {
      loading = true;
      frame.src = url;
    }
  }

  function external() {
    if (!url) return;
    if (native) nativeBrowser.external(url).catch(err);
    else window.open(url, '_blank', 'noopener');
  }

  $effect(() => {
    const next = app.browserUrl;
    if (next && next !== untrack(() => url)) untrack(() => go(next));
  });

  $effect(() => {
    if (!native || !opened) return;
    (app.overlay ? nativeBrowser.hide() : nativeBrowser.show()).catch(() => {});
  });

  onMount(() => {
    if (url && !native) {
      history = [url];
      pos = 0;
      loading = true;
    }
    if (!native || !host) return;
    const ro = new ResizeObserver(place);
    ro.observe(host);
    window.addEventListener('resize', place);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', place);
      if (opened) nativeBrowser.hide().catch(() => {});
    };
  });
</script>

<div class="browser">
  <div class="bar">
    <button class="icon-btn" aria-label={t('browser.back')} title={t('browser.back')} onclick={() => step(-1)}><ArrowLeft size={15} /></button>
    <button class="icon-btn" aria-label={t('browser.forward')} title={t('browser.forward')} onclick={() => step(1)}><ArrowRight size={15} /></button>
    <button class="icon-btn" aria-label={t('browser.reload')} title={t('browser.reload')} onclick={reload}>
      {#if loading}<span class="spin"><Loader size={15} /></span>{:else}<RotateCw size={14} />{/if}
    </button>
    <form onsubmit={(e) => (e.preventDefault(), go(input))}>
      <Globe size={13} />
      <input bind:value={input} placeholder={t('browser.url')} spellcheck="false" autocomplete="off" aria-label={t('browser.url')} onfocus={(e) => e.currentTarget.select()} />
    </form>
    <button class="icon-btn" aria-label={t('browser.external')} title={t('browser.external')} onclick={external} disabled={!url}><ExternalLink size={14} /></button>
  </div>
  <div class="view" bind:this={host}>
    {#if !url}
      <div class="empty">
        <Globe size={28} />
        <span>{t('browser.url')}</span>
      </div>
    {:else if !native}
      <iframe
        bind:this={frame}
        src={url}
        title={url}
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        onload={() => (loading = false)}
        onerror={() => (loading = false)}
      ></iframe>
      <div class="hint">{t('browser.unavailable')}</div>
    {/if}
  </div>
</div>

<style>
  .browser {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 40px;
    padding: 0 8px;
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
    gap: 8px;
    margin: 0 6px;
    padding: 0 10px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-3);
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
  .empty {
    height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 10px;
    color: var(--text-3);
  }
  .hint {
    position: absolute;
    left: 12px;
    bottom: 12px;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    color: var(--text-2);
    font-size: 12px;
    pointer-events: none;
  }
</style>
