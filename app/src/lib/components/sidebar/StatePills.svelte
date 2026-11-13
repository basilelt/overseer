<script lang="ts">
  import { t } from '../../i18n.svelte';
  import type { SessionState } from '../../session/session-state';
  import type { WorkspaceSummary } from '../../workspace/workspace-summary';

  let { summary, hideZero = false }: { summary: WorkspaceSummary; hideZero?: boolean } = $props();

  const PILLS: SessionState[] = ['attention', 'running', 'paused'];
</script>

{#each PILLS as s (s)}
  {#if summary.counts[s] > 0}<span class="pill" data-pill={s} style:--c="var(--state-{s})" title={t(`session.state.${s}`)}>{summary.counts[s]}</span>{/if}
{/each}
{#if summary.total || !hideZero}<span class="total">{summary.total}</span>{/if}

<style>
  .pill {
    flex: none;
    min-width: 18px;
    padding: 0 6px;
    border-radius: 9px;
    background: var(--c);
    color: #0b0b0c;
    font-size: 11px;
    font-weight: 700;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  .total {
    flex: none;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
</style>
