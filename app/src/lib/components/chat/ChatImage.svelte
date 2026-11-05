<script lang="ts">
  import ImageIcon from '@lucide/svelte/icons/image';
  import Modal from '../ui/Modal.svelte';
  import { chatImage } from '../../chat/image';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';

  let { lineId, at, i }: { lineId: string; at: number; i: number } = $props();

  let url = $state<string | null>(null);
  let failed = $state(false);
  let open = $state(false);
  const label = $derived(t('chat.images', { n: 1 }));

  // Chat rows use content-visibility, which would trap a fixed Modal inside the row.
  function portal(node: HTMLElement) {
    document.body.append(node);
    return { destroy: () => node.remove() };
  }

  $effect(() => {
    const client = app.client;
    url = null;
    failed = false;
    if (!client) return;
    let alive = true;
    let made: string | null = null;
    chatImage(client, lineId, at, i).then(
        (buf) => {
          if (alive) url = made = URL.createObjectURL(new Blob([buf]));
        },
        () => {
          if (alive) failed = true;
        },
      );
    return () => {
      alive = false;
      if (made) URL.revokeObjectURL(made);
    };
  });
</script>

{#if url}
  <button class="thumb" title={label} aria-label={label} onclick={() => (open = true)}><img src={url} alt={label} /></button>
  {#if open}
    <div use:portal>
      <Modal title={label} onclose={() => (open = false)} width={960}>
        <img class="full" src={url} alt={label} />
      </Modal>
    </div>
  {/if}
{:else}
  <span class="thumb placeholder faint" class:failed title={label}><ImageIcon size={16} /></span>
{/if}

<style>
  .thumb {
    display: grid;
    place-items: center;
    overflow: hidden;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .thumb img {
    display: block;
    max-width: 160px;
    max-height: 120px;
    object-fit: cover;
  }
  .placeholder {
    width: 48px;
    height: 48px;
  }
  .failed {
    opacity: 0.6;
  }
  .full {
    display: block;
    max-width: 100%;
    max-height: 75vh;
    margin: 0 auto;
    object-fit: contain;
  }
</style>
