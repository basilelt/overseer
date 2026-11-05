<script lang="ts">
  import { parseNotes, type Inline } from '../../release-notes';

  let { notes, empty, onlink }: { notes: string; empty: string; onlink: (href: string) => void } = $props();

  const blocks = $derived(parseNotes(notes));

  function follow(e: MouseEvent, href: string) {
    e.preventDefault();
    onlink(href);
  }
</script>

{#snippet inline(parts: Inline[])}
  {#each parts as p, i (i)}
    {#if p.t === 'text'}{p.v}{:else if p.t === 'code'}<code>{p.v}</code>{:else if p.t === 'strong'}<strong>{p.v}</strong>{:else if p.t === 'link'}<a href={p.href} onclick={(e) => follow(e, p.href)}>{p.v}</a>{:else}<a class="sha" href={p.href} title={p.href} onclick={(e) => follow(e, p.href)}>{p.v}</a>{/if}
  {/each}
{/snippet}

<div class="notes">
  {#each blocks as b, i (i)}
    {#if b.t === 'h'}
      <div class="h" class:top={b.level <= 2}>{@render inline(b.inl)}</div>
    {:else if b.t === 'ul'}
      <ul>
        {#each b.items as item, j (j)}<li>{@render inline(item)}</li>{/each}
      </ul>
    {:else}
      <p>{@render inline(b.inl)}</p>
    {/if}
  {:else}
    <p>{empty}</p>
  {/each}
</div>

<style>
  .notes {
    margin: 10px 0 12px;
    padding: 10px 12px;
    max-height: 240px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    color: var(--text-2);
    font-size: 12px;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }
  .notes .h {
    margin: 8px 0 2px;
    color: var(--text);
    font-weight: 600;
  }
  .notes .h:first-child {
    margin-top: 0;
  }
  .notes .h.top {
    font-size: 13px;
  }
  .notes ul {
    margin: 0 0 4px;
    padding-left: 16px;
  }
  .notes li {
    margin: 1px 0;
  }
  .notes p {
    margin: 0 0 4px;
  }
  .notes a {
    color: var(--accent);
    text-decoration: none;
  }
  .notes a:hover {
    text-decoration: underline;
  }
  .notes code {
    font-family: var(--mono);
    font-size: 11px;
    padding: 0 3px;
    border-radius: 3px;
    background: var(--surface-2);
  }
  .notes strong {
    color: var(--text);
    font-weight: 600;
  }
  .notes .sha {
    margin-left: 6px;
    padding: 0 4px;
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--text-2);
    font-family: var(--mono);
    font-size: 10px;
    vertical-align: 1px;
  }
  .notes .sha:hover {
    color: var(--text);
    text-decoration: none;
  }
</style>
