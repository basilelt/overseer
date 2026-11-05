<script lang="ts">
  import { untrack } from 'svelte';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { t } from '../../i18n.svelte';
  import { lingerOff } from '../../lifecycle';
  import { lifecycle, loadLifecycle, patchLifecycle } from '../../lifecycle.svelte';
  import type { AfterReboot } from '../../types';

  let { row = () => true, hit = () => false }: { row?: (id: string) => boolean; hit?: (id: string) => boolean } = $props();

  const modes: AfterReboot[] = ['agents', 'shells', 'nothing'];
  const s = $derived(lifecycle.status);
  const auto = $derived(s?.autostart);

  $effect(() => {
    untrack(() => loadLifecycle(true));
  });
</script>

{#if s && auto}
  <section>
    <h3>{t('settings.server')}</h3>
    {#if row('keepRunning')}
      {#if auto.supported}
        <label class="row" class:hit={hit('keepRunning')}>
          <span
            >{t('settings.keepRunning')}<small
              >{t('settings.keepRunningHint', { platform: auto.platform })}{auto.platform === 'launchd' ? ` ${t('settings.keepRunningMac')}` : ''}</small
            ></span
          >
          <input type="checkbox" class="switch" checked={auto.enabled} disabled={lifecycle.busy} onchange={(e) => patchLifecycle({ autostart: e.currentTarget.checked })} />
        </label>
        {#if lingerOff(s)}
          <div class="row warn">
            <span class="msg"><TriangleAlert size={13} />{t('settings.lingerOff', { command: auto.lingerCommand ?? '' })}</span>
            <button class="btn" disabled={lifecycle.busy} onclick={() => patchLifecycle({ linger: true })}>{t('settings.lingerEnable')}</button>
          </div>
        {/if}
      {:else}
        <p class="faint">{t('settings.keepRunningUnsupported')}</p>
      {/if}
    {/if}
    {#if row('afterReboot')}
      <label class="row stacked" class:hit={hit('afterReboot')}>
        <span>{t('settings.afterReboot')}<small>{t('settings.afterRebootHint')}</small></span>
        <select class="ctl" value={s.afterReboot} disabled={lifecycle.busy} onchange={(e) => patchLifecycle({ afterReboot: e.currentTarget.value as AfterReboot })}>
          {#each modes as m (m)}<option value={m}>{t(`settings.afterReboot.${m}`)}</option>{/each}
        </select>
      </label>
    {/if}
  </section>
{/if}

<style>
  section {
    padding: 10px 0 12px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
  }
  .row > span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  small {
    color: var(--text-3);
    font-size: 11px;
  }
  .row + .row {
    margin-top: 6px;
  }
  .warn > .msg {
    display: block;
    color: var(--text-2);
    font-size: 12px;
  }
  .warn > .msg :global(svg) {
    vertical-align: -2px;
    margin-right: 6px;
    color: var(--st-failed);
  }
  .warn .btn {
    flex-shrink: 0;
  }
  .row.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
  }
  .ctl {
    align-self: flex-start;
    width: 260px;
    max-width: 100%;
  }
  .row.hit {
    margin: 0 -8px;
    padding-left: 8px;
    padding-right: 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
</style>
