<script lang="ts">
  import { onMount } from 'svelte';
  import { apk, at, errorText } from '../../mobile/apk-update.svelte';

  let { row = () => true, hit = () => false }: { row?: (id: string) => boolean; hit?: (id: string) => boolean } = $props();

  const line = $derived.by(() => {
    switch (apk.phase) {
      case 'checking':
        return at('checking');
      case 'current':
        return at('current', { version: apk.current });
      case 'error':
        return apk.error ? errorText(apk.error) : '';
      default:
        return apk.info ? at('available', { version: apk.info.version }) : '';
    }
  });

  onMount(() => void apk.refreshStatus());
</script>

{#if apk.enabled}
  <section>
    <h3>{at(apk.updates ? 'section' : 'serverSection')}</h3>
    {#if apk.updates && row('apk')}
    <div class="row" class:hit={hit('apk')}>
      <span class="status">
        <span>{at('version')} <span class="mono">{apk.current || '—'}</span></span>
        {#if line}<small class:error={apk.phase === 'error'}>{line}</small>{/if}
      </span>
      <div class="actions">
        {#if apk.info && !apk.busy}<button class="btn" onclick={() => (apk.sheet = true)}>{at('view')}</button>{/if}
        <button class="btn" disabled={apk.busy} onclick={() => apk.check(true)}>{at('check')}</button>
      </div>
    </div>
    {/if}
    {#if row('apkToken')}
    <div class="row" class:hit={hit('apkToken')}>
      <span class="status">
        <span>{at('tokenTitle')}</span>
        <small>{apk.hasToken ? `${at('tokenSaved')} · ${at('tokenStored')}` : at('tokenNone')}</small>
        {#if !apk.updates}<small>{at('serverHint')}</small>{/if}
      </span>
      <div class="actions">
        {#if apk.hasToken}<button class="btn ghost" onclick={() => apk.removeToken()}>{at('tokenRemove')}</button>{/if}
        <button class="btn" onclick={() => apk.openToken()}>{apk.hasToken ? at('tokenReplace') : at('addToken')}</button>
      </div>
    </div>
    {/if}
  </section>
{/if}

<style>
  section {
    padding: 10px 0 12px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    min-height: 44px;
    padding: 4px 0;
  }
  .status {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1 1 180px;
  }
  small {
    color: var(--text-3);
    font-size: 11px;
    line-height: 1.45;
  }
  small.error {
    color: var(--st-failed);
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .actions .btn {
    min-height: 36px;
  }
  .row.hit {
    margin: 0 -8px;
    padding-left: 8px;
    padding-right: 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
</style>
