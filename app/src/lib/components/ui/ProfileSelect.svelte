<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { costIndex, formatIndex, NONE, resolveProfile } from '../../profiles';

  let {
    value,
    cwd,
    agent,
    onchange,
    inheritLabel,
  }: { value: string; cwd?: string; agent?: string; onchange: (id: string) => void; inheritLabel?: string } = $props();

  const store = $derived(app.profiles);
  const inherited = $derived(resolveProfile(store, { cwd }));
  const cost = (id: string) => {
    if (!agent || !store) return '';
    const x = formatIndex(costIndex(store.profiles.find((p) => p.id === id), agent, store.support[agent]));
    return x ? ` · ${x}` : '';
  };
</script>

{#if store}
  <select aria-label={t('profile.label')} {value} onchange={(e) => onchange(e.currentTarget.value)}>
    <option value="">{inheritLabel ?? t('profile.inherit', { name: inherited?.name ?? t('profile.none') })}</option>
    {#each store.profiles as p (p.id)}
      <option value={p.id}>{p.name}{cost(p.id)}</option>
    {/each}
    <option value={NONE}>{t('profile.none')}</option>
  </select>
{/if}
