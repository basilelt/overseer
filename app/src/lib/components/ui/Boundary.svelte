<script lang="ts">
  import type { Snippet } from 'svelte';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { t } from '../../i18n.svelte';

  let { name, children, onclose, width }: { name: string; children: Snippet; onclose?: () => void; width?: string } = $props();

  function report(error: unknown) {
    console.error(`[${name}] view crashed`, error);
  }

  const message = (error: unknown) => String((error as Error)?.message ?? error);
</script>

<svelte:boundary onerror={report}>
  {@render children()}
  {#snippet failed(error, reset)}
    <div class="crash" class:floating={!!onclose} class:fixed={!!width} style:width role="alert">
      <TriangleAlert size={22} strokeWidth={1.5} />
      <p>{t('boundary.failed')}</p>
      <p class="detail faint mono">{message(error)}</p>
      <div class="actions">
        <button class="btn primary" onclick={reset}>{t('common.retry')}</button>
        {#if onclose}
          <button
            class="btn"
            onclick={() => {
              onclose();
              reset();
            }}>{t('common.close')}</button>
        {/if}
      </div>
    </div>
  {/snippet}
</svelte:boundary>

<style>
  .crash {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
    color: var(--text);
    background: var(--bg);
  }
  .crash.fixed {
    flex: none;
  }
  .crash.floating {
    position: fixed;
    z-index: 80;
    left: 50%;
    top: 30%;
    transform: translateX(-50%);
    width: min(420px, calc(100vw - 32px));
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.25);
  }
  p {
    margin: 0;
  }
  .detail {
    max-width: 100%;
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
