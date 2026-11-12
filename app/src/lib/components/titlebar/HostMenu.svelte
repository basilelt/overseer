<script lang="ts">
  import Monitor from '@lucide/svelte/icons/monitor';
  import Server from '@lucide/svelte/icons/server';
  import { t } from '../../i18n.svelte';
  import { settings } from '../../settings/settings.svelte';
  import { app } from '../../state.svelte';
  import type { HostTarget } from '../../types';
  import { connectHow } from '../../host/host-choice';
  import { hostColor, hostName } from '../../host/hosts';
  import { hostKey, openWindow } from '../../window';

  let { tip, onclose }: { tip: string; onclose: () => void } = $props();
  const conn = $derived(app.conn);
  const many = $derived(app.hosts.length > 1);
  const here = $derived(new Set(app.hosts.map((l) => l.key)));

  function pick(h: HostTarget) {
    onclose();
    const how = connectHow(settings, h, { active: hostKey(conn.target), keys: [...here], live: app.connected });
    if (how === 'ask') app.open('connect', `ask:${JSON.stringify(h)}`);
    else if (how === 'window') void openWindow(h);
    else if (how === 'switch') void app.switchHost(hostKey(h));
    else app.connect(h, how === 'add' ? 'add' : 'replace');
  }

  async function disconnect() {
    onclose();
    await app.disconnect();
  }
</script>

<div class="menu" role="menu">
  <div class="status faint">{tip}</div>
  {#if many}
    {#each app.hosts as l (l.key)}
      <button role="menuitemradio" aria-checked={l === app.link} class:on={l === app.link} onclick={() => (onclose(), app.switchHost(l.key))}>
        <span class="dot" style:background={hostColor(l.key)}></span>{hostName(l.target)}
      </button>
    {/each}
    <hr />
  {/if}
  {#if !app.mobile && !here.has('local')}
    <button role="menuitem" onclick={() => pick({ kind: 'local', label: 'Local' })}>
      <Monitor size={13} />
      {t('connect.local')}
    </button>
  {/if}
  {#each settings.recentHosts.filter((h) => !many || !here.has(hostKey(h))) as h (h.host)}
    <button role="menuitem" onclick={() => pick(h)}><Server size={13} />{h.label}</button>
  {/each}
  <hr />
  <button role="menuitem" onclick={() => app.open('connect')}>{t('host.connect')}</button>
  {#if conn.phase === 'connected' && conn.target.kind !== 'local'}
    <button role="menuitem" onclick={disconnect}>{t('host.disconnect')}</button>
  {/if}
</div>

<style>
  .menu {
    position: absolute;
    top: 32px;
    right: 0;
    min-width: 220px;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    z-index: 40;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border-radius: 4px;
    text-align: left;
  }
  .menu button:hover {
    background: var(--surface-hover);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
  }
  .menu .on {
    font-weight: 600;
    color: var(--text);
  }
  .status {
    padding: 4px 8px 6px;
    font-size: 12px;
  }
  hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
</style>
