<script lang="ts">
  import FilePen from '@lucide/svelte/icons/file-pen';

  let { files }: { files: string[] } = $props();
</script>

<ul>
  {#each files as f (f)}
    <li title={f}>
      <FilePen size={12} />
      <span class="name">{f.split('/').pop()}</span>
      <span class="dir faint mono">{f.split('/').slice(0, -1).join('/')}</span>
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 0;
    font-size: 12px;
    color: var(--text-3);
    min-width: 0;
  }
  .name {
    color: var(--text);
    flex: none;
  }
  .dir {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
