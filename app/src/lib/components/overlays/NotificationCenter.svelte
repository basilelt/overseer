<script lang="ts">
  import type { Component } from 'svelte';
  import Bell from '@lucide/svelte/icons/bell';
  import BellOff from '@lucide/svelte/icons/bell-off';
  import Settings from '@lucide/svelte/icons/settings';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Coffee from '@lucide/svelte/icons/coffee';
  import Download from '@lucide/svelte/icons/download';
  import EthernetPort from '@lucide/svelte/icons/ethernet-port';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import MessageCircleQuestionMark from '@lucide/svelte/icons/message-circle-question-mark';
  import Unplug from '@lucide/svelte/icons/unplug';
  import Upload from '@lucide/svelte/icons/upload';
  import Modal from '../ui/Modal.svelte';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { center, runEntry } from '../../notify/notifications.svelte';
  import type { EntryKind } from '../../notify/notifications';
  import { age } from '../../time';
  import { isTargetMuted, toggleTargetMute } from '../../notify/os-notify';

  let { onclose }: { onclose: () => void } = $props();

  const icons: Record<EntryKind, Component<{ size?: number }>> = {
    needsInput: MessageCircleQuestionMark,
    taskTesting: FlaskConical,
    finished: CircleCheck,
    failed: CircleAlert,
    allIdle: Coffee,
    portForwarded: EthernetPort,
    portClosed: Unplug,
    update: Download,
    upload: Upload,
    error: CircleAlert,
  };
  const host = $derived(app.conn.target.kind === 'local' ? t('host.local') : app.conn.target.label || app.conn.target.host || '');
  const now = Date.now();
</script>

<Modal title={t('center.title')} {onclose} width={440}>
  <div class="head">
    <span class="faint">{host}</span>
    <span class="acts">
      <button class="btn ghost" disabled={!center.unread} onclick={() => center.markAllRead()}>{t('center.markAllRead')}</button>
      <button class="btn ghost" disabled={!center.entries.length} onclick={() => center.clear()}>{t('center.clear')}</button>
      <button class="icon-btn" aria-label={t('nb.board')} title={t('nb.board')} onclick={() => app.open('settings', 'notifications')}><Settings size={14} /></button>
    </span>
  </div>
  {#if center.entries.length}
    <ul class="list">
      {#each center.entries as e (e.id)}
        {@const Icon = icons[e.kind] ?? Bell}
        <li>
          <button class="item" class:unread={!e.read} class:bad={e.kind === 'error' || e.kind === 'failed'} onclick={() => runEntry(app, e)}>
            <span class="ic"><Icon size={15} /></span>
            <span class="text">
              <span class="title">{e.title}</span>
              {#if e.body || e.source}<span class="sub faint">{[e.source, e.body].filter(Boolean).join(' · ')}</span>{/if}
            </span>
            <span class="when faint">{age(new Date(e.at).toISOString(), now)}</span>
            {#if !e.read}<span class="dot" aria-label={t('center.unread')}></span>{/if}
          </button>
          {#if e.action?.kind === 'line'}
            {@const key = e.action.key}
            {@const muted = isTargetMuted(app, key)}
            <button class="icon-btn mute" class:on={muted} aria-label={t(muted ? 'nb.unmuteSession' : 'nb.muteSession')} title={t(muted ? 'nb.unmuteSession' : 'nb.muteSession')} onclick={() => toggleTargetMute(app, key)}><BellOff size={13} /></button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty faint">{t('center.empty')}</p>
  {/if}
</Modal>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 6px;
    font-size: 12px;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .acts .btn {
    height: 24px;
    font-size: 12px;
  }
  .list {
    list-style: none;
    margin: 0 -8px;
    padding: 0;
  }
  .list li {
    position: relative;
    display: flex;
    align-items: flex-start;
  }
  .mute {
    flex: none;
    margin-top: 6px;
    color: var(--text-3);
    opacity: 0;
  }
  .list li:hover .mute,
  .mute:focus-visible,
  .mute.on {
    opacity: 1;
  }
  .mute.on {
    color: var(--accent);
  }
  :global(.phone) .mute {
    opacity: 1;
  }
  .item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 8px;
    border-radius: var(--radius);
    text-align: left;
    color: var(--text-2);
  }
  .item:hover {
    background: var(--surface-hover);
  }
  .ic {
    flex: none;
    margin-top: 1px;
    color: var(--text-3);
  }
  .bad .ic {
    color: var(--st-failed);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title {
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .unread .title {
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    flex: none;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    margin-top: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .empty {
    margin: 24px 0;
    text-align: center;
  }
  :global(.phone) .item {
    min-height: 52px;
    padding: 10px 8px;
  }
</style>
