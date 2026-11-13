<script lang="ts">
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import { t } from '../../i18n.svelte';
  import type { SidebarItem } from '../../session/grouping';
  import { modeOf, setMode } from '../../chat/viewmode.svelte';

  let { item }: { item: SidebarItem } = $props();
  const mode = $derived(modeOf(item));
</script>

<div class="seg" role="group" aria-label={t('chat.view')} data-tour="view-toggle">
  <button class:on={mode === 'chat'} aria-pressed={mode === 'chat'} title={t('chat.chat')} onclick={() => setMode(item, 'chat')}><MessageSquare size={13} /><span>{t('chat.chat')}</span></button>
  <button class:on={mode === 'terminal'} aria-pressed={mode === 'terminal'} title={t('chat.terminal')} onclick={() => setMode(item, 'terminal')}><SquareTerminal size={13} /><span>{t('chat.terminal')}</span></button>
</div>

<style>
  .seg {
    display: flex;
    padding: 2px;
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  button {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 22px;
    padding: 0 7px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--text-3);
    font: 11.5px var(--font);
    cursor: pointer;
  }
  button:hover {
    color: var(--text-2);
  }
  button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
</style>
