<script lang="ts">
  import { gesture } from '../../links';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { LocalLine } from '../../types';

  // Pause, resume, interrupt and close live in the chat header (SessionBar); only Pin and the PR stay here.
  let { line }: { line: LocalLine } = $props();

  let busy = $state(false);

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
    }
  }

  const pin = () => act(line.local?.pinned ? 'action.unpin' : 'action.pin', () => app.api.pin(line.line.id, !line.local?.pinned));
</script>

<div class="actions">
  <button class="btn" disabled={busy} onclick={pin}>{line.local?.pinned ? t('action.unpin') : t('action.pin')}</button>
  {#if line.line.prUrl}
    <button class="btn" onclick={(e) => app.openUrl(line.line.prUrl!, undefined, gesture(e))}><GitPullRequest size={13} />{t('action.openPr')}</button>
    <button class="btn ghost" title={t('action.openBrowser')} onclick={(e) => app.openUrl(line.line.prUrl!, undefined, gesture(e))}><ExternalLink size={13} /></button>
  {/if}
</div>

<style>
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 14px 12px;
  }
</style>
