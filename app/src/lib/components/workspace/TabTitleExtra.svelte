<script lang="ts">
  import TabFullTitle from './TabFullTitle.svelte';
  import TabTitleInput from './TabTitleInput.svelte';

  let {
    tabKey,
    name,
    edit,
    ondone,
  }: { tabKey: string; name: string; edit: { initial: string; openedAt: number } | null; ondone: () => void } = $props();
</script>

{#if edit}
  <TabTitleInput {tabKey} initial={edit.initial} openedAt={edit.openedAt} {ondone} />
{:else}
  <TabFullTitle {name} />
{/if}
