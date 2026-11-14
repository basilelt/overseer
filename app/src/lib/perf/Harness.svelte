<script lang="ts">
  import { onMount } from 'svelte';
  import { setClient } from '../daemon/client';
  import { MockDaemon } from '../daemon/mock';
  import { startPresence } from '../presence';
  import { startRest } from '../daemon/rest';
  import { app } from '../state.svelte';
  import Sidebar from '../components/sidebar/Sidebar.svelte';
  import TerminalPane from '../components/workspace/TerminalPane.svelte';
  import { PERF_TERMS, STREAM_EVERY_MS, streamBlock } from './stream';

  const streamAt = Number(import.meta.env.VITE_PERF_STREAM_AT_MS || 45_000);
  const mock = new MockDaemon({ live: false, delayMs: 0 });
  setClient(mock);

  let ids = $state<string[]>([]);

  onMount(() => {
    const stopPresence = startPresence();
    const stopRest = startRest();
    let feeder: ReturnType<typeof setInterval> | undefined;
    let seq = 0;
    const start = setTimeout(() => {
      feeder = setInterval(() => {
        if (seq % 20 === 0) window.dispatchEvent(new Event('pointermove'));
        if (ids[0]) mock.feed(ids[0], streamBlock(seq++));
      }, STREAM_EVERY_MS);
    }, streamAt);
    (async () => {
      await app.init();
      const live = app.data.terms.filter((t) => !t.exited).map((t) => t.id);
      while (live.length < PERF_TERMS) live.push((await app.api.createTerm({ cols: 80, rows: 24 })).id);
      ids = live.slice(0, PERF_TERMS);
    })();
    return () => {
      stopPresence();
      stopRest();
      clearTimeout(start);
      clearInterval(feeder);
    };
  });
</script>

<div class="perf">
  <Sidebar />
  <div class="grid">
    {#each ids as id, i (id)}
      <div class="cell"><TerminalPane termId={id} focused={i === 0} /></div>
    {/each}
  </div>
</div>

<style>
  .perf {
    display: flex;
    height: 100vh;
  }
  .grid {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: 1fr;
    min-width: 0;
  }
  .cell {
    display: flex;
    min-height: 0;
    min-width: 0;
  }
</style>
