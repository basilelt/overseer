<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { parseBody, previewSuggestion } from '../../review/threads';

  let { body, code, onapply, applying = false }: { body: string; code?: string; onapply?: () => void; applying?: boolean } = $props();

  const blocks = $derived(parseBody(body));
  const codeLines = $derived(code === undefined ? [] : code.split('\n'));
</script>

<div class="body">
  {#each blocks as b, i (i)}
    {#if b.type === 'p'}
      <p>{#each b.inlines as x, j (j)}{#if x.code}<code>{x.text}</code>{:else}{x.text}{/if}{/each}</p>
    {:else if b.type === 'code'}
      <pre class="code mono">{b.text}</pre>
    {:else}
      <div class="sugg">
        <div class="shead">
          <span>{t('review.suggestion')}</span>
          {#if onapply}<button class="btn small" disabled={applying} onclick={onapply}>{t('review.apply')}</button>{/if}
        </div>
        <div class="mono lines">
          {#each previewSuggestion(codeLines, b.lines) as l, j (j)}
            <div class="sl {l.kind}"><span class="sign">{l.kind === 'add' ? '+' : l.kind === 'del' ? '-' : ' '}</span>{l.text}</div>
          {/each}
        </div>
      </div>
    {/if}
  {/each}
</div>

<style>
  .body {
    font-size: 12.5px;
    line-height: 1.55;
    user-select: text;
    -webkit-user-select: text;
    overflow-wrap: anywhere;
  }
  p {
    margin: 0 0 6px;
    white-space: pre-wrap;
  }
  p:last-child {
    margin-bottom: 0;
  }
  code {
    font-family: var(--mono);
    font-size: 11.5px;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .code {
    margin: 4px 0 6px;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    font-size: 11.5px;
    overflow-x: auto;
  }
  .sugg {
    margin: 4px 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .shead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 4px 4px 10px;
    font-size: 11.5px;
    color: var(--text-2);
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    min-height: 30px;
  }
  .small {
    height: 22px;
    font-size: 11.5px;
    padding: 0 8px;
  }
  .lines {
    font-size: 11.5px;
    overflow-x: auto;
  }
  .sl {
    white-space: pre;
    padding: 0 10px 0 0;
    min-width: max-content;
  }
  .sl.add {
    background: var(--diff-add-bg);
  }
  .sl.del {
    background: var(--diff-del-bg);
  }
  .sign {
    display: inline-block;
    width: 22px;
    text-align: center;
    color: var(--text-3);
    user-select: none;
    -webkit-user-select: none;
  }
  .add .sign {
    color: var(--diff-add);
  }
  .del .sign {
    color: var(--diff-del);
  }
</style>
