<script lang="ts">
  import type { Snippet } from 'svelte';
  import { watchNear } from '../../review/window';

  let { height, pinned = false, children }: { height: number; pinned?: boolean; children: Snippet } = $props();

  let el: HTMLDivElement | undefined = $state();
  let near = $state(false);
  let measured = $state<number | null>(null);
  const shown = $derived(near || pinned);

  $effect.pre(() => {
    void height;
    measured = null;
  });

  $effect(() => {
    if (!el) return;
    const node = el;
    return watchNear(node, (v) => {
      if (!v && shown) measured = node.offsetHeight;
      near = v;
    });
  });
</script>

<div bind:this={el} style:height={shown ? undefined : `${measured ?? height}px`}>
  {#if shown}{@render children()}{/if}
</div>
