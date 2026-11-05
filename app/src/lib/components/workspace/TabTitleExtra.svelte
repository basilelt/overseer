<script lang="ts">
  import TabTitleInput from './TabTitleInput.svelte';

  let {
    tabKey,
    edit,
    ondone,
  }: { tabKey: string; edit: { initial: string; openedAt: number } | null; ondone: () => void } = $props();
</script>

{#if edit}
  <TabTitleInput {tabKey} initial={edit.initial} openedAt={edit.openedAt} {ondone} />
{/if}
