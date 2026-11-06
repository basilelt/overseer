<script lang="ts">
  import Square from '@lucide/svelte/icons/square';
  import Pause from '@lucide/svelte/icons/pause';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Play from '@lucide/svelte/icons/play';
  import PhoneOff from '@lucide/svelte/icons/phone-off';
  import { t, type Key } from '../../i18n.svelte';
  import type { SessionAction, SessionView } from '../../session-state';

  let { view, acting = null, confirmClose = false, onact }: { view: SessionView; acting?: SessionAction | null; confirmClose?: boolean; onact: (a: SessionAction) => void } = $props();

  const LABEL: Record<SessionAction, Key> = {
    interrupt: 'session.action.interrupt',
    pause: 'session.action.pause',
    cancelPause: 'session.action.cancelPause',
    resume: 'session.action.resume',
    close: 'session.action.close',
  };
  // Pause and cancel pause share a slot: only one makes sense at a time.
  const shown = $derived((['interrupt', view.actions.cancelPause.enabled ? 'cancelPause' : 'pause', 'resume', 'close'] as const));

  const off = (a: SessionAction) => acting !== null || !view.actions[a].enabled;
  const tip = (a: SessionAction) => {
    const why = view.actions[a].reason;
    return a === 'close' && confirmClose ? t('chat.closeConfirm') : why && !view.actions[a].enabled ? `${t(LABEL[a])}: ${t(why)}` : t(LABEL[a]);
  };
</script>

{#each shown as a (a)}
  <button
    class="icon-btn"
    class:confirm={a === 'close' && confirmClose}
    class:text={a === 'resume' && view.actions.resume.enabled}
    data-action={a}
    title={tip(a)}
    aria-label={t(LABEL[a])}
    disabled={off(a)}
    onclick={() => onact(a)}
  >
    {#if a === 'interrupt'}<Square size={14} />
    {:else if a === 'pause'}<Pause size={15} />
    {:else if a === 'cancelPause'}<Undo2 size={15} />
    {:else if a === 'resume'}<Play size={14} />
    {:else}<PhoneOff size={15} />{/if}
    {#if a === 'resume' && view.actions.resume.enabled}<span class="cc">{t('session.action.resume')}</span>{/if}
    {#if a === 'close' && confirmClose}<span class="cc">{t('chat.closeConfirm')}</span>{/if}
  </button>
{/each}

<style>
  .confirm,
  .text {
    display: inline-flex;
    align-items: center;
    white-space: nowrap;
  }
  .confirm {
    color: var(--st-failed);
    width: auto;
    gap: 4px;
    padding: 0 6px;
  }
  .text {
    width: auto;
    gap: 4px;
    padding: 0 8px;
    color: var(--text);
  }
  .cc {
    font-size: 11px;
  }
  button:disabled {
    opacity: 0.4;
  }
</style>
