<script lang="ts">
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { beside } from '../../review/beside.svelte';
  import { focusPane, update, workspace } from '../../workspace.svelte';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { GROUP_COLOR } from '../../ws-color';

  const items = $derived(app.scopeItems().filter((i) => i.lineId || i.termId));

  function pick(key: string) {
    const i = workspace.layout.panes.findIndex((p) => !p.tabs.includes(beside.key ?? ''));
    if (i >= 0 && i !== workspace.layout.focused) update((l) => focusPane(l, i));
    app.select(key);
  }
</script>

<nav class="rail" aria-label={t('live.sessions')}>
  {#each items as it (it.key)}
    <button class="s" class:on={it.key === app.selectedKey} title={it.title} aria-label={it.title} aria-current={it.key === app.selectedKey ? 'true' : undefined} onclick={() => pick(it.key)}>
      <AgentGlyph agent={it.kind} size={15} />
      <span class="dot" style:background={GROUP_COLOR[it.group]}></span>
    </button>
  {/each}
</nav>

<style>
  .rail {
    width: 52px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 0;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
    overflow-y: auto;
    scrollbar-width: none;
  }
  .s {
    position: relative;
    width: 34px;
    height: 34px;
    flex: none;
    display: grid;
    place-items: center;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface);
    color: var(--text-2);
    cursor: pointer;
  }
  .s:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .s.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .dot {
    position: absolute;
    right: -3px;
    top: -3px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 2px solid var(--bg-subtle);
  }
</style>
