<script lang="ts">
  import { t } from '../../i18n.svelte';
  import type { AgentInfo } from '../../types';

  let { agent, value, onchange, label }: { agent: AgentInfo | undefined; value: string; onchange: (v: string) => void; label: string } = $props();

  const CUSTOM = '__custom__';
  const models = $derived(agent?.models ?? []);
  let custom = $state(false);
  const showInput = $derived(!models.length || custom || (value !== '' && !models.includes(value)));
  const selected = $derived(showInput && models.length ? CUSTOM : value);

  $effect(() => {
    agent?.id;
    custom = false;
  });

  function pick(v: string) {
    custom = v === CUSTOM;
    onchange(custom ? '' : v);
  }

  const focus = (el: HTMLInputElement) => {
    if (models.length) el.focus();
  };
</script>

<div class="model">
  {#if models.length}
    <select aria-label={label} value={selected} onchange={(e) => pick(e.currentTarget.value)}>
      <option value="">{t('model.default')}</option>
      {#each models as m (m)}<option value={m}>{m}</option>{/each}
      <option value={CUSTOM}>{t('model.custom')}</option>
    </select>
  {/if}
  {#if showInput}
    <input
      class="mono"
      aria-label={label}
      {value}
      placeholder={t('model.default')}
      spellcheck="false"
      autocomplete="off"
      use:focus
      oninput={(e) => onchange(e.currentTarget.value.trim())}
    />
  {/if}
</div>

<style>
  .model {
    display: flex;
    gap: 6px;
    min-width: 0;
  }
  select,
  input {
    flex: 1;
    min-width: 0;
  }
</style>
