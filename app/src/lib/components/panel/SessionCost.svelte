<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { formatCost, formatTokens } from '../../usage/format';
  import type { SessionCost } from '../../types';

  let { cost }: { cost: SessionCost } = $props();

  const time = (iso: string) => new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
</script>

<div class="cost">
  {#each cost.models as m (m.model)}
    <div class="row">
      <span class="model mono" title={m.model}>{m.model}</span>
      <span class="usd mono">{formatCost(m.cost)}</span>
    </div>
    <p class="split faint mono">
      {t('cost.split', { input: formatTokens(m.input), write: formatTokens(m.cacheWrite), read: formatTokens(m.cacheRead), output: formatTokens(m.output) })}
    </p>
  {/each}
  {#each cost.switches as s (s.at + s.to)}
    <p class="switch small">{t('cost.switch', { time: time(s.at), from: s.from, to: s.to, cost: formatCost(s.cost) })}</p>
  {/each}
  {#if cost.partial}<p class="faint small">{t('cost.partial')}</p>{/if}
  <p class="faint small">{t('cost.hint', { date: cost.pricesAsOf })}</p>
</div>

<style>
  .cost {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    min-width: 0;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }
  .model {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .usd {
    flex: none;
  }
  .split {
    margin: 0 0 4px;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .switch {
    margin: 2px 0;
    color: var(--text-2);
  }
  .small {
    margin: 4px 0 0;
    font-size: 11px;
  }
</style>
