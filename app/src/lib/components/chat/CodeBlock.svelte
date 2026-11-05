<script lang="ts">
  import Copy from '@lucide/svelte/icons/copy';
  import Check from '@lucide/svelte/icons/check';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { cachedHighlight, highlightCode } from '../../review/highlight';
  import { watchNear } from '../../review/window';

  let { code, lang = '' }: { code: string; lang?: string } = $props();

  let html = $state<string | null>(null);
  let copied = $state(false);
  let el = $state<HTMLDivElement>();

  // Highlight only blocks near the viewport; remounts reuse the cached HTML.
  $effect(() => {
    const c = code;
    const l = lang;
    const node = el;
    let live = true;
    html = l ? cachedHighlight(c, l) : null;
    if (!node || html !== null || !l || c.length >= 60_000) return;
    let started = false;
    const stop = watchNear(node, (near) => {
      if (!near || started) return;
      started = true;
      highlightCode(c, l).then((h) => live && (html = h));
    });
    return () => {
      live = false;
      stop();
    };
  });

  async function copy() {
    try {
      await app.client?.copyText(code);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }
</script>

<div class="code" bind:this={el}>
  <div class="bar">
    <span class="lang">{lang}</span>
    <button class="copy" title={t('chat.copy')} aria-label={t('chat.copy')} onclick={copy}>
      {#if copied}<Check size={13} />{:else}<Copy size={13} />{/if}
    </button>
  </div>
  {#if html !== null}
    <pre class="hljs"><code>{@html html}</code></pre>
  {:else}
    <pre><code>{code}</code></pre>
  {/if}
</div>

<style>
  .code {
    margin: 6px 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 4px 2px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-3);
    user-select: none;
    -webkit-user-select: none;
  }
  .copy {
    display: grid;
    place-items: center;
    min-width: 28px;
    min-height: 26px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text-2);
    cursor: pointer;
  }
  .copy:hover {
    background: var(--surface-hover);
  }
  pre {
    margin: 0;
    padding: 8px 10px;
    overflow-x: auto;
    font: 12px/1.5 var(--mono);
    color: var(--text);
  }
  .code {
    --hl-keyword: #a626a4;
    --hl-string: #2f7a3a;
    --hl-number: #b45309;
    --hl-comment: #8b8b94;
    --hl-title: #2557c7;
    --hl-type: #0e7490;
  }
  :global([data-theme='dark']) .code {
    --hl-keyword: #d19bf0;
    --hl-string: #9fd59a;
    --hl-number: #f0b471;
    --hl-comment: #6e6e78;
    --hl-title: #8fb4ff;
    --hl-type: #73cfe0;
  }
  .hljs :global(.hljs-keyword),
  .hljs :global(.hljs-built_in),
  .hljs :global(.hljs-literal) {
    color: var(--hl-keyword);
  }
  .hljs :global(.hljs-string),
  .hljs :global(.hljs-regexp),
  .hljs :global(.hljs-attr) {
    color: var(--hl-string);
  }
  .hljs :global(.hljs-number) {
    color: var(--hl-number);
  }
  .hljs :global(.hljs-comment) {
    color: var(--hl-comment);
    font-style: italic;
  }
  .hljs :global(.hljs-title),
  .hljs :global(.hljs-section) {
    color: var(--hl-title);
  }
  .hljs :global(.hljs-type),
  .hljs :global(.hljs-name),
  .hljs :global(.hljs-tag) {
    color: var(--hl-type);
  }
</style>
