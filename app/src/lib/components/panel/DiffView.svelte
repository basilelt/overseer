<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { parseDiff, tokenize, type DiffFile, type DiffLine } from '../../diff';
  import { t } from '../../i18n.svelte';
  import { chunks } from '../../review/window';
  import Lazy from '../ui/Lazy.svelte';

  let { diff }: { diff: string } = $props();

  const files = $derived(parseDiff(diff));
  let toggled = $state<Record<string, boolean>>({});

  const isOpen = (path: string, size: number) => toggled[path] ?? size <= 300;
  const size = (f: (typeof files)[number]) => f.hunks.reduce((n, h) => n + h.lines.length, 0);
  const flat = (f: DiffFile): (DiffLine | { header: string })[] => f.hunks.flatMap((h) => [{ header: h.header }, ...h.lines]);
</script>

<div class="diff">
  {#if !files.length}
    <p class="faint empty">{t('panel.diffEmpty')}</p>
  {/if}
  {#each files as f (f.path)}
    {@const open = isOpen(f.path, size(f))}
    <div class="file">
      <button class="fhead" aria-expanded={open} onclick={() => (toggled[f.path] = !open)}>
        <span class="chev" class:open><ChevronRight size={12} /></span>
        <span class="path mono">{f.path}</span>
        <span class="add">+{f.added}</span>
        <span class="del">−{f.removed}</span>
      </button>
      {#if open}
        {@const rows = flat(f)}
        <div class="body mono">
          {#each chunks(rows.length, 50, (i) => ('header' in rows[i] ? 22 : 18)) as ch, ci (ci)}
            <Lazy height={ch.height}>
              {#each rows.slice(ch.start, ch.end) as l, li (li)}
                {#if 'header' in l}
                  <div class="hunk">{l.header}</div>
                {:else}
                  <div class="ln {l.kind}">
                    <span class="no">{l.oldNo ?? ''}</span>
                    <span class="no">{l.newNo ?? ''}</span>
                    <span class="sign">{l.kind === 'add' ? '+' : l.kind === 'del' ? '-' : ' '}</span>
                    <span class="code"
                      >{#if l.kind === 'meta'}{l.text}{:else}{#each tokenize(l.text) as tok, ti (ti)}<span class={tok.kind}>{tok.text}</span>{/each}{/if}</span
                    >
                  </div>
                {/if}
              {/each}
            </Lazy>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .diff {
    display: flex;
    flex-direction: column;
    gap: 8px;
    user-select: text;
    -webkit-user-select: text;
  }
  .empty {
    margin: 4px 0;
  }
  .file {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .fhead {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    background: var(--surface-2);
    font-size: 12px;
    text-align: left;
  }
  .chev {
    display: inline-grid;
    transition: transform var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .add {
    color: var(--diff-add);
  }
  .del {
    color: var(--diff-del);
  }
  .body {
    font-size: 11.5px;
    line-height: 1.55;
    overflow-x: auto;
  }
  .hunk {
    padding: 2px 8px;
    color: var(--diff-hunk);
    background: var(--bg-subtle);
    white-space: pre;
  }
  .ln {
    display: flex;
    white-space: pre;
    min-width: max-content;
  }
  .ln.add {
    background: var(--diff-add-bg);
  }
  .ln.del {
    background: var(--diff-del-bg);
  }
  .ln.meta {
    color: var(--text-3);
  }
  .no {
    width: 36px;
    flex: none;
    padding-right: 6px;
    text-align: right;
    color: var(--text-3);
    user-select: none;
    -webkit-user-select: none;
  }
  .sign {
    width: 14px;
    flex: none;
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
  .code {
    padding-right: 12px;
  }
  .kw {
    color: var(--st-testing);
  }
  .str {
    color: var(--st-working);
  }
  .num {
    color: var(--diff-hunk);
  }
  .com {
    color: var(--text-3);
    font-style: italic;
  }
</style>
