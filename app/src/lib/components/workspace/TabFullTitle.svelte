<script lang="ts">
  let { name }: { name: string } = $props();
</script>

<span class="full" aria-hidden="true"><span class="name">{name}</span></span>

<style>
  /* A cut title is laid over the strip instead of widening the tab, so no tab moves under the
     pointer; being inside the tab, it keeps its hover, click and drag. The delay skips tabs the
     pointer only passes over. It ends where the cut title ends and grows leftwards, and has no close
     button: the tab's own one stays where it was. */
  .full {
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
    z-index: 2;
    display: flex;
    align-items: center;
    height: 34px;
    max-width: 400px;
    padding-left: 10px;
    border-left: 1px solid var(--border);
    background: var(--surface-hover);
    animation: reveal 0s 300ms backwards;
  }
  @keyframes reveal {
    from {
      visibility: hidden;
    }
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.tab.active) .full {
    background: var(--bg);
  }
  :global(.tabbar.dragging) .full {
    visibility: hidden;
  }
</style>
