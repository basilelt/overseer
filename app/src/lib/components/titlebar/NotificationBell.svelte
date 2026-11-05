<script lang="ts">
  import Bell from '@lucide/svelte/icons/bell';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { center } from '../../notifications.svelte';

  const label = $derived(center.unread ? `${t('center.title')} · ${t('center.unreadCount', { n: center.unread })}` : t('center.title'));
</script>

<button class="icon-btn bell" class:active={app.overlay === 'notifications'} title={label} aria-label={label} onclick={() => app.open(app.overlay === 'notifications' ? null : 'notifications')}>
  <Bell size={15} />
  {#if center.unread}<span class="badge">{center.unread > 99 ? '99+' : center.unread}</span>{/if}
</button>

<style>
  .bell {
    position: relative;
  }
  .badge {
    position: absolute;
    top: -2px;
    right: -4px;
    min-width: 15px;
    height: 15px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-fg);
    font-size: 10px;
    font-weight: 600;
    line-height: 15px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
</style>
