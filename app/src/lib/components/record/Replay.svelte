<script lang="ts">
  import { activeAt, clock, type Recording } from '../../record';
  import { rt } from '../../record.svelte';

  let { recording }: { recording: Recording } = $props();

  let url = $state<string | null>(null);
  let player = $state<HTMLVideoElement | null>(null);
  let list = $state<HTMLOListElement | null>(null);
  let now = $state(0);
  const active = $derived(activeAt(recording.events, now));

  function seek(at: number) {
    now = at;
    if (player) {
      player.currentTime = at / 1000;
      void player.play()?.catch(() => {});
    }
  }

  function probe(e: Event) {
    const v = e.currentTarget as HTMLVideoElement;
    if (Number.isFinite(v.duration)) return;
    v.addEventListener('durationchange', () => (v.currentTime = 0), { once: true });
    v.currentTime = 1e9;
  }

  $effect(() => {
    const i = active;
    if (i < 0 || !list) return;
    (list.children[i] as HTMLElement | undefined)?.scrollIntoView?.({ block: 'nearest' });
  });

  $effect(() => {
    const video = recording.video;
    if (!video) return void (url = null);
    const u = URL.createObjectURL(video);
    url = u;
    return () => URL.revokeObjectURL(u);
  });
</script>

<div class="replay">
  {#if url}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={player} src={url} controls preload="metadata" onloadedmetadata={probe} ontimeupdate={(e) => (now = e.currentTarget.currentTime * 1000)}></video>
  {:else}
    <p class="faint small">{rt('noVideoReplay')}</p>
  {/if}
  <ol class="events" bind:this={list} aria-label={rt('timeline')}>
    {#each recording.events as ev, i (i)}
      <li>
        <button class:on={i === active} aria-current={i === active ? 'true' : undefined} onclick={() => seek(ev.at)}>
          <span class="mono time">{clock(ev.at)}</span>
          <span class="kind kind-{ev.kind}">{ev.kind}</span>
          <span class="text">{ev.text}</span>
        </button>
      </li>
    {/each}
  </ol>
</div>

<style>
  .replay {
    display: grid;
    gap: 8px;
  }
  video {
    width: 100%;
    max-height: 280px;
    border-radius: var(--radius);
    background: #000;
  }
  .small {
    font-size: 11px;
    margin: 0;
  }
  .events {
    list-style: none;
    margin: 0;
    padding: 4px;
    max-height: 180px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .events button {
    display: flex;
    gap: 8px;
    align-items: baseline;
    width: 100%;
    padding: 3px 6px;
    border-radius: 4px;
    font-size: 11px;
    text-align: left;
    color: var(--text-2);
  }
  .events button:hover {
    background: var(--surface);
  }
  .events button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .mono {
    font-family: var(--mono);
  }
  .time {
    flex: none;
    width: 3.2em;
  }
  .kind {
    flex: none;
    width: 3.6em;
    color: var(--text-3);
  }
  .kind-error,
  .kind-toast {
    color: var(--st-failed);
  }
  .text {
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
