<script lang="ts">
  import { untrack } from 'svelte';
  import Square from '@lucide/svelte/icons/square';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../../state.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import { clock } from '../../record';
  import { recorder, rt } from '../../record.svelte';

  $effect(() => {
    const v = app.mobile || ui.layout === 'phone' ? ui.tab : app.view;
    untrack(() => recorder.note('view', rt('view', { view: v })));
  });

  $effect(() => {
    const o = app.overlay;
    if (o) untrack(() => recorder.note('view', rt('overlay', { overlay: o })));
  });
</script>

<div class="pill" class:phone={app.mobile || ui.layout === 'phone'} data-rec-skip role="status">
  <span class="dot" aria-hidden="true"></span>
  <span class="mono">{rt('recording', { time: clock(recorder.elapsed) })}</span>
  <span class="faint">· {rt('actions', { n: recorder.count })}</span>
  <button class="btn primary stop" onclick={() => void recorder.stop()}><Square size={11} fill="currentColor" />{rt('stop')}</button>
  <button class="icon-btn" aria-label={rt('discard')} title={rt('discard')} onclick={() => recorder.cancel()}><X size={14} /></button>
</div>

<style>
  .pill {
    position: fixed;
    left: 50%;
    bottom: calc(16px + env(safe-area-inset-bottom, 0px));
    transform: translateX(-50%);
    z-index: 900;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 12px;
    max-width: calc(100vw - 32px);
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.18);
    font-size: 12px;
    white-space: nowrap;
  }
  .pill.phone {
    bottom: calc(72px + env(safe-area-inset-bottom, 0px));
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--st-failed);
  }
  .mono {
    font-family: var(--mono);
  }
  .stop {
    border-radius: 999px;
    padding: 2px 10px;
  }
</style>
