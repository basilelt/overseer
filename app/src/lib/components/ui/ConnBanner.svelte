<script lang="ts">
  import { untrack } from 'svelte';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Unplug from '@lucide/svelte/icons/unplug';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Server from '@lucide/svelte/icons/server';
  import Bell from '@lucide/svelte/icons/bell';
  import Power from '@lucide/svelte/icons/power';
  import { keepRunningOffer } from '../../lifecycle';
  import { keepRunningAnswered, lifecycle, loadLifecycle, patchLifecycle } from '../../lifecycle.svelte';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { serverBanner } from '../../server';
  import { hooksOffer } from '../../hooks';
  import { hooks, hooksLater, hooksNever, loadHooks, setHooks } from '../../hooks.svelte';
  import { settings } from '../../settings.svelte';
  import { isDemo } from '../../demo';

  const demo = isDemo();

  const conn = $derived(app.conn);
  let dismissed = $state<string | null>(null);
  let restarting = $state(false);
  const update = $derived(serverBanner(conn, dismissed));
  const offer = $derived(hooksOffer(hooks.status, conn, settings.hooksNever, hooks.later));
  const keep = $derived(keepRunningOffer(lifecycle.status, conn, settings.keepRunningOffered ?? []));

  $effect(() => {
    if (conn.phase === 'connected' && app.connected)
      untrack(() => {
        loadHooks();
        if (conn.target.kind !== 'local') loadLifecycle();
      });
  });

  function keepRunning(on: boolean) {
    keepRunningAnswered();
    if (on) void patchLifecycle({ autostart: true });
  }

  async function restart() {
    restarting = true;
    try {
      await app.restartServer();
    } catch {}
    restarting = false;
  }
</script>

{#if !demo}
{#if conn.prompt && app.overlay !== 'connect'}
  <div class="banner" role="alert">
    <KeyRound size={14} />
    <span class="msg"><strong>{t('auth.needed')}</strong> · {t(`auth.${conn.prompt.kind}.title`)} · {conn.target.label}</span>
    <button class="btn primary" onclick={() => app.open('connect')}>{t('auth.review')}</button>
  </div>
{:else if conn.phase === 'error'}
  <div class="banner error" role="alert">
    <TriangleAlert size={14} />
    <span class="msg" title={conn.error}><strong>{t('conn.error')}</strong> · {conn.error?.split('\n').find((l) => l.trim()) ?? ''}</span>
    <button class="btn" onclick={() => app.connect(conn.target)}>{t('common.retry')}</button>
    <button class="btn ghost" onclick={() => app.open('connect')}>{t('host.connect')}</button>
  </div>
{:else if conn.phase === 'idle' && app.client}
  <div class="banner" role="status">
    <Unplug size={14} />
    <span class="msg"><strong>{t('conn.idle')}</strong> · {conn.target.kind === 'local' ? t('host.local') : conn.target.label}</span>
    <button class="btn" onclick={() => app.connect(conn.target)}>{t('conn.reconnect')}</button>
    <button class="btn ghost" onclick={() => app.open('connect')}>{t('host.connect')}</button>
  </div>
{:else if conn.phase === 'reconnecting'}
  <div class="banner" role="status">
    <LoaderCircle size={14} class="spin" />
    <span class="msg">{t('conn.reconnecting', { n: conn.attempt ?? 1 })}</span>
  </div>
{:else if update}
  <div class="banner" role="status">
    <Server size={14} />
    <span class="msg" title={t('server.running', { running: update.running, reason: update.reason })}>{t(update.key, { version: update.version, n: update.n })}</span>
    <button class="btn primary" disabled={restarting} onclick={restart}>{t('server.restart')}</button>
    <button class="btn ghost" onclick={() => (dismissed = update.id)}>{t('server.later')}</button>
  </div>
{:else if keep}
  <div class="banner" role="status">
    <Power size={14} />
    <span class="msg wrap" title={t('lifecycle.offerHint')}>{t('lifecycle.offer', { host: conn.target.label })}</span>
    <button class="btn primary" disabled={lifecycle.busy} onclick={() => keepRunning(true)}>{t('lifecycle.keep')}</button>
    <button class="btn" onclick={() => keepRunning(false)}>{t('lifecycle.notNow')}</button>
  </div>
{:else if offer}
  <div class="banner" role="status" data-tour="hooks">
    <Bell size={14} />
    <span class="msg" title={offer.map((a) => a.path ?? a.name).join('\n')}>{t('hooks.offer', { agents: offer.map((a) => a.name).join(', ') })}</span>
    <button class="btn primary" disabled={hooks.busy} onclick={() => setHooks(offer.map((a) => a.agent), true)}>{t('hooks.install')}</button>
    <button class="btn" onclick={hooksLater}>{t('hooks.notNow')}</button>
    <button class="btn ghost" onclick={hooksNever}>{t('hooks.never')}</button>
  </div>
{/if}
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--text-2);
    font-size: 12px;
  }
  .error {
    color: var(--st-failed);
  }
  .msg {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .msg.wrap {
    white-space: normal;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
  }
  .msg strong {
    font-weight: 600;
  }
  .btn {
    height: 24px;
    font-size: 12px;
  }
</style>
