<script lang="ts">
  import { untrack } from 'svelte';
  import { titleEdit } from '../../tabs/titles';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';

  let { tabKey, initial, openedAt, ondone }: { tabKey: string; initial: string; openedAt: number; ondone: () => void } = $props();

  let draft = $state(untrack(() => initial));
  let done = false;

  // Activating a tab focuses its terminal a moment later; win focus back instead of committing.
  function blur(e: FocusEvent) {
    const el = e.currentTarget as HTMLInputElement;
    if (performance.now() - openedAt < 500) setTimeout(() => !done && el.isConnected && el.focus());
    else finish(true);
  }

  function finish(save: boolean) {
    if (done) return;
    done = true;
    ondone();
    const item = app.items.find((i) => i.key === tabKey);
    const next = save && item ? titleEdit(item, draft) : undefined;
    if (next !== undefined) void app.renameItem(tabKey, next);
  }

  function focusSelect(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<input
  class="name-input"
  aria-label={t('tabs.renameLabel')}
  maxlength="80"
  bind:value={draft}
  use:focusSelect
  onpointerdown={(e) => e.stopPropagation()}
  onclick={(e) => e.stopPropagation()}
  onauxclick={(e) => e.stopPropagation()}
  onkeydown={(e) => {
    e.stopPropagation();
    if (e.key === 'Enter') finish(true);
    else if (e.key === 'Escape') finish(false);
  }}
  onblur={blur}
/>

<style>
  .name-input {
    flex: 1 1 auto;
    min-width: 0;
    width: 100%;
    height: 22px;
    padding: 0 4px;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--accent);
    border-radius: 4px;
    outline: none;
  }
</style>
