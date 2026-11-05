<script lang="ts">
  import { app } from '../../state.svelte';
  import { UsageFeed } from '../../usage/store.svelte';
  import { formatCost, formatTokens } from '../../usage/format';
  import { ust } from '../../usage/strings';

  let { lineId }: { lineId: string } = $props();

  const feed = new UsageFeed();
  const session = $derived(app.connected && app.conn.phase === 'connected' && app.client ? `${app.conn.target.kind}:${app.conn.target.host ?? ''}` : null);
  const u = $derived(feed.data?.groups[0]?.usage ?? null);

  $effect(() => {
    if (!session || !app.client) return;
    return feed.use(session, app.client, (fn) => app.onChange((_p, _n, e) => fn(e)), { range: '30d', by: 'session', line: lineId });
  });
</script>

{#if u && u.total > 0}
  <p class="usage faint mono" title={`${ust('last30')} · ${ust('costHint')}`}>
    {ust('sessionLine', { tokens: formatTokens(u.total), cost: formatCost(u.cost, u.costPartial), n: u.messages })}
  </p>
{/if}

<style>
  .usage {
    margin: 2px 0 0;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
