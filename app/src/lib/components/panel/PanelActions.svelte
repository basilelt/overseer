<script lang="ts">
  import { gesture } from '../../links';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import Pause from '@lucide/svelte/icons/pause';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import X from '@lucide/svelte/icons/x';
  import type { SidebarItem } from '../../grouping';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { LocalLine } from '../../types';

  let { item, line }: { item: SidebarItem; line: LocalLine } = $props();

  let confirming = $state(false);
  let busy = $state(false);
  const lineState = $derived(line.line.state);

  async function act(label: Key, fn: () => Promise<unknown>) {
    busy = true;
    try {
      await fn();
      app.notify(t('action.done', { action: t(label) }));
      await app.refresh();
    } catch (e) {
      app.notify(String((e as Error).message ?? e), 'error');
    } finally {
      busy = false;
      confirming = false;
    }
  }

  const hold = () => act('action.hold', () => app.api.hold(line.line.id));
  const close = () => (confirming ? act('action.close', () => app.api.close(line.line.id)) : (confirming = true));
  const resume = () => act('action.resume', () => app.resumeLine(item));
  const pin = () => act(line.local?.pinned ? 'action.unpin' : 'action.pin', () => app.api.pin(line.line.id, !line.local?.pinned));
</script>

<div class="actions">
  {#if lineState === 'held'}
    <button class="btn" disabled={busy} onclick={resume}><RotateCcw size={13} />{t('action.resume')}</button>
  {:else}
    <button class="btn" disabled={busy || !!line.local?.pinned} onclick={hold}><Pause size={13} />{t('action.hold')}</button>
  {/if}
  <button class="btn" disabled={busy} onclick={pin}>{line.local?.pinned ? t('action.unpin') : t('action.pin')}</button>
  {#if line.line.prUrl}
    <button class="btn" onclick={(e) => app.openUrl(line.line.prUrl!, undefined, gesture(e))}><GitPullRequest size={13} />{t('action.openPr')}</button>
    <button class="btn ghost" title={t('action.openBrowser')} onclick={(e) => app.openUrl(line.line.prUrl!, undefined, gesture(e))}><ExternalLink size={13} /></button>
  {/if}
  <button class="btn danger" class:confirming disabled={busy} onclick={close} onblur={() => (confirming = false)}>
    <X size={13} />{confirming ? t('common.confirm') : t('action.close')}
  </button>
  {#if confirming}<p class="hint faint">{t('action.confirmClose')}</p>{/if}
</div>

<style>
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 14px 12px;
  }
  .confirming {
    border-color: var(--st-failed);
  }
  .hint {
    width: 100%;
    margin: 0;
    font-size: 12px;
  }
</style>
