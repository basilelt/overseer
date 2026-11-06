<script lang="ts">
  import { onMount } from 'svelte';
  import { attention } from '../../attention';
  import { byState } from '../../by-state';
  import { t } from '../../i18n.svelte';
  import { saveSettings, settings } from '../../settings.svelte';
  import { filterItems, hostFor, toggleOpen, visibleIds } from '../../spaces';
  import { app } from '../../state.svelte';
  import { TIME_BUCKETS } from '../../time-bucket';
  import { hostKey } from '../../window';
  import { itemMenu } from '../../wsactions';
  import { visibleClock } from '../../visible-clock';
  import AttentionBanner from './AttentionBanner.svelte';
  import StateGroups from './StateGroups.svelte';

  let { filter }: { filter: string } = $props();

  let now = $state(Date.now());
  onMount(() => visibleClock((n) => (now = n)));

  const q = $derived(filter.trim());
  const names = $derived(new Map(app.ws.workspaces.map((w) => [w.id, w.name])));
  const items = $derived(filterItems(app.items, (key) => names.get(app.itemWs.get(key) ?? '') ?? '', q));
  const sections = $derived(byState(items, app.itemWs, now, visibleIds(app.ws.workspaces)));
  const banner = $derived(attention(items, app.itemWs));
  const host = $derived(hostFor(app.conn));
  const saved = $derived(settings.statesOpen?.[hostKey(app.conn.target)]);
  const opened = $derived(new Set(saved ?? []));
  const empty = $derived(!banner.length && sections.every((s) => !s.count));

  function toggle(bucket: string) {
    saveSettings({ statesOpen: { ...settings.statesOpen, [hostKey(app.conn.target)]: toggleOpen(saved, bucket, new Set(TIME_BUCKETS), '') } });
  }

  function select(key: string, e?: MouseEvent) {
    if (e && (e.metaKey || e.ctrlKey || e.shiftKey)) {
      const next = new Set(app.multi);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      if (app.selectedKey && !next.size) next.add(app.selectedKey);
      app.multi = next;
      return;
    }
    app.multi = new Set();
    app.select(key);
  }

  const menu = (key: string, title: string, e: MouseEvent) => app.openMenu(e, title, itemMenu(key));
</script>

<div class="states">
  <AttentionBanner entries={banner} onselect={(e) => app.select(e.key)} />
  {#each sections as s (s.state)}
    {#if s.count}
      <section class="section">
        <h3 class="head">{t(`ws.section.${s.state}`)}<span class="n">{s.count}</span></h3>
        {#if s.state === 'closed'}
          {#each s.buckets as b (b.bucket)}
            <button class="bucket" type="button" aria-expanded={opened.has(b.bucket)} onclick={() => toggle(b.bucket)}>
              <span class="chev" aria-hidden="true">{opened.has(b.bucket) ? '▾' : '▸'}</span>
              <span class="bname">{t(`ws.bucket.${b.bucket}`)}</span>
              <span class="n">{b.count}</span>
            </button>
            {#if opened.has(b.bucket)}
              <StateGroups groups={b.groups} {names} {now} {host} selectedKey={app.selectedKey} onselect={select} onmenu={menu} />
            {/if}
          {/each}
        {:else}
          <StateGroups groups={s.groups} {names} {now} {host} selectedKey={app.selectedKey} onselect={select} onmenu={menu} />
        {/if}
      </section>
    {/if}
  {/each}
  {#if empty}<p class="none faint">{q ? t('common.none') : t('sidebar.empty')}</p>{/if}
</div>

<style>
  .states {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 0 8px;
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 12px 0;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .n {
    font-size: 11px;
    font-weight: 500;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .bucket {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    margin: 0 6px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    font-weight: 600;
  }
  .bucket:hover {
    background: var(--surface-hover);
  }
  .bname {
    flex: 1;
  }
  .chev {
    width: 10px;
    color: var(--text-2);
  }
  .none {
    padding: 24px 12px;
    text-align: center;
  }
</style>
