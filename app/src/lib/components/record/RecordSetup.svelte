<script lang="ts">
  import Circle from '@lucide/svelte/icons/circle';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { feedback } from '../../feedback.svelte';
  import { canVideo, MAX_MS } from '../../record';
  import { recorder, rt } from '../../record.svelte';

  const supported = canVideo();
  const min = MAX_MS / 60_000;
  let video = $state(supported);

  function start() {
    recorder
      .start({ video: supported && video, onDone: (recording) => feedback.show({ kind: 'bug', recording }) })
      .catch((e) => app.notify(rt('denied', { error: String((e as Error)?.message ?? e) }), 'error'));
  }

  const close = () => recorder.dismiss();
</script>

<Modal title={rt('setupTitle')} onclose={close} width={460}>
  <div class="setup">
    <p class="small">{rt('setupBody')}</p>
    {#if supported}
      <label class="check">
        <input type="checkbox" class="switch" bind:checked={video} />
        <span>{rt('video')}<small>{rt('videoHint', { min })}</small></span>
      </label>
    {:else}
      <p class="faint small">{rt('noVideo')}</p>
    {/if}
    <p class="faint small">{rt('limit', { min })}</p>
  </div>
  {#snippet footer()}
    <span class="spacer"></span>
    <button class="btn" onclick={close}>{rt('cancel')}</button>
    <button class="btn primary" onclick={start} disabled={recorder.phase === 'starting'}><Circle size={12} fill="currentColor" />{rt('start')}</button>
  {/snippet}
</Modal>

<style>
  .setup {
    display: grid;
    gap: 12px;
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
  .faint.small {
    font-size: 11px;
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 12px;
  }
  .check span {
    display: grid;
  }
  .check small {
    color: var(--text-3);
    font-size: 11px;
  }
  .spacer {
    flex: 1;
  }
</style>
