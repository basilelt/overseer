<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { isMac } from '../../platform';
  import { settings } from '../../settings.svelte';
  import { sheetRows } from '../../shortcuts';

  let { onclose }: { onclose: () => void } = $props();

  const sections = $derived(sheetRows({ mac: isMac, ctrlWClosesTab: settings.ctrlWClosesTab }));
</script>

<Modal title={t('keys.title')} {onclose} width={560}>
  {#each sections as s (s.id)}
    <section>
      <h3>{t(`keys.section.${s.id}` as Key)}</h3>
      {#each s.rows as row (row.id)}
        <div class="row">
          <span>{t(`keys.${row.id}` as Key)}</span>
          <span class="keys">
            {#each row.keys as k, i}
              {#if i > 0}<span class="or">{t('keys.or')}</span>{/if}
              <kbd>{k.label}</kbd>
              {#if k.outsideTerminal}<small>{t('keys.outsideTerminal')}</small>{/if}
            {/each}
          </span>
        </div>
      {/each}
    </section>
  {/each}
</Modal>

<style>
  section {
    padding: 8px 0 10px;
    border-bottom: 1px solid var(--border);
  }
  section:last-child {
    border-bottom: 0;
  }
  h3 {
    margin: 0 0 6px;
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
    min-height: 28px;
    font-size: 12.5px;
  }
  .keys {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
  }
  kbd {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-2);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    white-space: nowrap;
  }
  .or,
  small {
    color: var(--text-3);
    font-size: 11px;
  }
</style>
