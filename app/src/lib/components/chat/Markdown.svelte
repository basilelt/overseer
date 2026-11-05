<script lang="ts">
  import { getContext } from 'svelte';
  import { parseMarkdownCached } from '../../chat/markdown';
  import { openFile } from '../../preview';
  import CodeBlock from './CodeBlock.svelte';

  let { text }: { text: string } = $props();
  const blocks = $derived(parseMarkdownCached(text));
  const lineOf = getContext<(() => string) | undefined>('chat-line');

  function onclick(e: MouseEvent) {
    const a = (e.target as Element).closest<HTMLElement>('a[data-file]');
    if (!a?.dataset.file) return;
    e.preventDefault();
    openFile(a.dataset.file, undefined, lineOf?.());
  }
</script>

<div class="md" role="presentation" {onclick}>
  {#each blocks as b, i (i)}
    {#if b.type === 'p'}
      <p>{@html b.html}</p>
    {:else if b.type === 'h'}
      <p class="h h{Math.min(b.level, 3)}">{@html b.html}</p>
    {:else if b.type === 'list'}
      {#if b.ordered}
        <ol start={b.start}>{#each b.items as it, j (j)}<li>{@html it}</li>{/each}</ol>
      {:else}
        <ul>{#each b.items as it, j (j)}<li>{@html it}</li>{/each}</ul>
      {/if}
    {:else if b.type === 'quote'}
      <blockquote>{@html b.html}</blockquote>
    {:else if b.type === 'code'}
      <CodeBlock code={b.code} lang={b.lang} />
    {:else if b.type === 'hr'}
      <hr />
    {:else if b.type === 'table'}
      <div class="table">
        <table>
          <thead><tr>{#each b.head as c, j (j)}<th>{@html c}</th>{/each}</tr></thead>
          <tbody>{#each b.rows as r, k (k)}<tr>{#each r as c, j (j)}<td>{@html c}</td>{/each}</tr>{/each}</tbody>
        </table>
      </div>
    {/if}
  {/each}
</div>

<style>
  .md {
    min-width: 0;
    overflow-wrap: anywhere;
    line-height: 1.55;
  }
  .md > :first-child {
    margin-top: 0;
  }
  .md > :last-child {
    margin-bottom: 0;
  }
  p,
  ul,
  ol,
  blockquote {
    margin: 6px 0;
  }
  ul,
  ol {
    padding-left: 22px;
  }
  .h {
    font-weight: 600;
    margin-top: 12px;
  }
  .h1 {
    font-size: 1.15em;
  }
  .h2 {
    font-size: 1.07em;
  }
  blockquote {
    padding-left: 10px;
    border-left: 3px solid var(--border-strong);
    color: var(--text-2);
  }
  hr {
    border: 0;
    border-top: 1px solid var(--border);
  }
  .md :global(code) {
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--surface-2);
    font: 0.9em var(--mono);
  }
  .md :global(pre code) {
    padding: 0;
    background: none;
    font: inherit;
  }
  .md :global(a) {
    color: var(--accent);
  }
  .md :global(.file-link) {
    cursor: pointer;
    text-decoration: underline;
  }
  .table {
    overflow-x: auto;
    margin: 6px 0;
  }
  table {
    border-collapse: collapse;
    font-size: 0.95em;
  }
  th,
  td {
    padding: 4px 8px;
    border: 1px solid var(--border);
    text-align: left;
  }
  th {
    background: var(--surface-2);
    font-weight: 600;
  }
</style>
