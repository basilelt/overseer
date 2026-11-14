<script lang="ts">
  import type { Snippet } from 'svelte';

  let { header, actions, children }: { header: Snippet; actions?: Snippet; children: Snippet } = $props();
</script>

<section class="frame">
  <div class="head">
    <div class="name">{@render header()}</div>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </div>
  <div class="body">{@render children()}</div>
</section>

<style>
  .frame {
    border: 1px solid var(--ws-frame-border);
    border-radius: 10px;
    background: var(--ws-frame-bg);
    margin: 0 6px;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0 3px 3px;
  }
  :global(:root.phone) .frame {
    border-radius: 14px;
  }
  :global(:root.phone) .head {
    min-height: var(--touch);
  }
  :global(:root.phone) .body {
    gap: 4px;
  }
</style>
