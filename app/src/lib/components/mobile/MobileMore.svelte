<script module lang="ts">
  import Server from '@lucide/svelte/icons/server';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Unplug from '@lucide/svelte/icons/unplug';
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right';
  import Network from '@lucide/svelte/icons/network';
  import Activity from '@lucide/svelte/icons/activity';
  import { st } from '../../system/strings';
  import Settings from '@lucide/svelte/icons/settings';
  import Search from '@lucide/svelte/icons/search';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Download from '@lucide/svelte/icons/download';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import MessageSquarePlus from '@lucide/svelte/icons/message-square-plus';
  import Compass from '@lucide/svelte/icons/compass';
  import WandSparkles from '@lucide/svelte/icons/wand-sparkles';
  import Bell from '@lucide/svelte/icons/bell';
  import { center } from '../../notify/notifications.svelte';
  import { tt } from '../../tour/strings';
  import { ft } from '../../feedback/feedback.svelte';
  import Clapperboard from '@lucide/svelte/icons/clapperboard';
  import { rt } from '../../record/record.svelte';
  import Gauge from '@lucide/svelte/icons/gauge';
  import { ust } from '../../usage/strings';
  import GitPullRequestArrow from '@lucide/svelte/icons/git-pull-request-arrow';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import { gt } from '../../github/strings';
  import { dt } from '../../diag/diag.svelte';
  import type { Component } from 'svelte';
  import { connLabel, hostChipDot, HOST_WORKING_DOT } from '../../conn-label';
  import { t } from '../../i18n.svelte';
  import { saveSettings, settings } from '../../settings/settings.svelte';
  import type { ThemePref } from '../../theme/theme';
  import { app } from '../../state.svelte';
  import { command } from '../../commands/commands';
  import { registerMore, type CommandId } from '../../commands/commands-more';
  import { apk, at, pillLabel } from '../../mobile/apk-update.svelte';
  import RotateCw from '@lucide/svelte/icons/rotate-cw';

  registerMore();

  export const tools: { id: CommandId; icon: Component<{ size?: number }>; label: () => string; badge?: () => number }[] = [
    { id: 'notifications', icon: Bell, label: () => t('center.title'), badge: () => center.unread },
    { id: 'ports', icon: Network, label: () => t('ports.title') },
    { id: 'system', icon: Activity, label: () => st('title') },
    { id: 'usage', icon: Gauge, label: () => ust('title') },
    { id: 'graph', icon: GitBranch, label: () => t('tabs.gitGraph') },
    { id: 'prs', icon: GitPullRequestArrow, label: () => gt('title') },
    { id: 'settings', icon: Settings, label: () => t('title.settings') },
    { id: 'palette', icon: Search, label: () => t('mobile.more.search') },
    { id: 'logs', icon: ScrollText, label: () => dt('show') },
    { id: 'feedback', icon: MessageSquarePlus, label: () => ft('title') },
    { id: 'record', icon: Clapperboard, label: () => rt('palette') },
    { id: 'organize', icon: WandSparkles, label: () => t('org.title') },
    { id: 'tour', icon: Compass, label: () => tt('again') },
  ];
</script>

<script lang="ts">
  const conn = $derived(app.conn);
  const phase = $derived(connLabel(conn));
  const hostDot = $derived(hostChipDot(conn.phase));
  const host = $derived(conn.target.kind === 'local' ? t('host.local') : conn.target.label || t('mobile.more.notConnected'));
  let confirmRestart = $state(false);
  let restarting = $state(false);

  async function restart() {
    if (!confirmRestart) return void (confirmRestart = true);
    restarting = true;
    try {
      await app.restartServer();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
    restarting = false;
    confirmRestart = false;
  }

  const themes: { id: ThemePref; label: () => string }[] = [
    { id: 'auto', label: () => t('settings.theme.system') },
    { id: 'light', label: () => t('settings.theme.light') },
    { id: 'dark', label: () => t('settings.theme.dark') },
  ];

</script>

<div class="more">
  <h3>{t('mobile.more.connection')}</h3>
  <div class="card">
    <div class="host">
      <Server size={18} />
      <span class="text">
        <span class="title">{host}</span>
        <span class="sub faint">{conn.resolved ?? phase}{conn.version && conn.phase === 'connected' ? ` · v${conn.version}` : ''}</span>
      </span>
      <span class="dot" class:pulse={hostDot === HOST_WORKING_DOT} style:background={hostDot}></span>
    </div>
    <button class="row" onclick={() => app.open('connect')}><ArrowLeftRight size={18} /><span class="label">{t('mobile.more.switchHost')}</span><ChevronRight size={16} /></button>
    <button class="row" onclick={() => app.open('connect', 'devicekey')}><KeyRound size={18} /><span class="label">{t('mobile.more.deviceKey')}</span><ChevronRight size={16} /></button>
    {#if app.connected && conn.server}
      <button class="row" class:danger={confirmRestart} disabled={restarting} onclick={restart}
        ><RotateCw size={18} /><span class="text"
          ><span class="label">{t(confirmRestart ? 'server.restartConfirm' : 'server.restartTo', { version: conn.server.installed })}</span><span class="sub faint wrap"
            >{t(conn.server.terms === 0 ? 'server.updateIdle' : conn.server.terms === 1 ? 'server.updateOne' : 'server.update', { version: conn.server.installed, n: conn.server.terms })}</span
          ></span
        ></button
      >
    {/if}
    {#if app.connected}
      <button class="row danger" onclick={() => app.disconnect()}><Unplug size={18} /><span class="label">{t('host.disconnect')}</span></button>
    {/if}
  </div>

  <h3>{t('mobile.more.tools')}</h3>
  <div class="card">
    {#each tools.filter((tool) => command(tool.id).available()) as tool (tool.id)}
      <button class="row" onclick={() => command(tool.id).run()}><tool.icon size={18} /><span class="label">{tool.label()}</span>{#if tool.badge?.()}<span class="badge">{tool.badge()}</span>{/if}<ChevronRight size={16} /></button>
    {/each}
    {#if __SELF_UPDATE__ && apk.enabled && apk.updates}
      {@const pending = pillLabel(apk)}
      <button class="row" onclick={() => (pending ? (apk.sheet = true) : apk.check(true))}>
        <Download size={18} />
        <span class="label">{at('more')}</span>
        {#if pending}<span class="badge">{pending}</span>{:else if apk.current}<span class="faint">{apk.current}</span>{/if}
        <ChevronRight size={16} />
      </button>
    {/if}
  </div>

  <h3>{t('mobile.more.appearance')}</h3>
  <div class="seg themes" role="radiogroup" aria-label={t('settings.theme')}>
    {#each themes as th (th.id)}
      <button role="radio" aria-checked={settings.theme === th.id} class:on={settings.theme === th.id} onclick={() => saveSettings({ theme: th.id })}>{th.label()}</button>
    {/each}
  </div>
</div>

<style>
  .more {
    flex: 1;
    overflow-y: auto;
    padding: 8px 16px 24px;
  }
  h3 {
    margin: 16px 4px 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    overflow: hidden;
  }
  .host,
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 52px;
    padding: 8px 14px;
    text-align: left;
    color: var(--text-2);
  }
  .row {
    border-top: 1px solid var(--border);
  }
  .card > .row:first-child {
    border-top: 0;
  }
  .row:active:not(:disabled) {
    background: var(--surface-active);
  }
  .row:disabled {
    opacity: 0.5;
  }
  .label,
  .title {
    flex: 1;
    min-width: 0;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex-shrink: 0;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 12px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .danger,
  .danger .label {
    color: var(--st-failed);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .sub {
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub.wrap {
    white-space: normal;
  }
  .seg {
    display: flex;
    padding: 3px;
    gap: 3px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-2);
  }
  .themes {
    flex-wrap: wrap;
  }
  .seg.themes button {
    flex-basis: 28%;
    white-space: nowrap;
  }
  .seg button {
    flex: 1;
    min-height: 40px;
    border-radius: var(--radius);
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
</style>
