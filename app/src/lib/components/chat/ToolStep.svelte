<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import FilePen from '@lucide/svelte/icons/file-pen';
  import FilePlus from '@lucide/svelte/icons/file-plus';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import FileText from '@lucide/svelte/icons/file-text';
  import Search from '@lucide/svelte/icons/search';
  import Bot from '@lucide/svelte/icons/bot';
  import Globe from '@lucide/svelte/icons/globe';
  import Wrench from '@lucide/svelte/icons/wrench';
  import { getContext, type Component } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { clip, hasDetail, miniDiff, toolSummary } from '../../chat/tools';
  import { fileKind } from '../../files';
  import { openFile } from '../../preview';
  import type { ChatTool, ToolKind } from '../../types';
  import { basename } from '../../paths';

  let { tool }: { tool: ChatTool } = $props();
  const lineOf = getContext<(() => string) | undefined>('chat-line');

  const ICONS: Record<ToolKind, Component<{ size?: number }>> = { edit: FilePen, write: FilePlus, bash: SquareTerminal, read: FileText, search: Search, task: Bot, web: Globe, other: Wrench };
  let open = $state(false);
  let full = $state(false);
  const Icon = $derived(ICONS[tool.kind] ?? Wrench);
  const diff = $derived(open ? miniDiff(tool.diff) : null);
  const out = $derived(open && tool.output ? (full ? { text: tool.output, more: 0 } : clip(tool.output)) : null);
  const expandable = $derived(hasDetail(tool));
  const file = $derived(tool.path && !toolSummary(tool).includes(basename(tool.path)) ? basename(tool.path) : '');
</script>

<div class="step" class:open>
  <button class="head" disabled={!expandable} aria-expanded={open} onclick={() => (open = !open)}>
    <span class="st {tool.state}" title={t(`chat.state.${tool.state}`)}></span>
    <span class="ic"><Icon size={13} /></span>
    <span class="sum">{toolSummary(tool)}</span>
    {#if file}<span class="file faint">{file}</span>{/if}
    {#if tool.kind === 'task' && tool.detail}<span class="file faint">{tool.detail}</span>{/if}
    {#if expandable}<span class="chev"><ChevronRight size={13} /></span>{/if}
  </button>
  {#if open}
    <div class="body">
      {#if tool.path && fileKind(tool.path)}<button class="path link" onclick={() => openFile(tool.path!, undefined, lineOf?.())}>{tool.path}</button>{:else if tool.path}<div class="path faint">{tool.path}</div>{/if}
      {#if tool.command}<pre class="cmd">$ {tool.command}</pre>{/if}
      {#if diff}
        <div class="diff">
          <div class="stat"><span class="add">+{diff.added}</span> <span class="del">−{diff.removed}</span></div>
          {#each diff.lines as l, i (i)}
            <div class="dl {l.kind}"><span class="sg">{l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ' '}</span>{l.text}</div>
          {/each}
          {#if diff.more}<div class="more faint">{t('chat.moreLines', { n: diff.more })}</div>{/if}
        </div>
      {/if}
      {#if out}
        <pre class="out">{out.text}</pre>
        {#if out.more || tool.truncated}
          <button class="more link" onclick={() => (full = !full)} disabled={!out.more && !full}>
            {out.more ? t('chat.moreLines', { n: out.more }) : t('chat.truncated')}
          </button>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .step {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    min-height: 30px;
    padding: 3px 8px;
    border: 0;
    background: none;
    color: var(--text-2);
    font: 12.5px var(--font);
    text-align: left;
    cursor: pointer;
  }
  .head:disabled {
    cursor: default;
  }
  .head:not(:disabled):hover {
    background: var(--surface-hover);
  }
  .st {
    flex-shrink: 0;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--st-done);
  }
  .st.running {
    background: var(--st-working);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .st.failed {
    background: var(--st-failed);
  }
  .ic {
    display: grid;
    color: var(--text-3);
  }
  .sum {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .file {
    flex-shrink: 0;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    display: grid;
    margin-left: auto;
    color: var(--text-3);
    transition: transform var(--ease);
  }
  .open .chev {
    transform: rotate(90deg);
  }
  .body {
    padding: 4px 8px 8px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .path {
    font: 11px var(--mono);
    overflow-wrap: anywhere;
  }
  button.path {
    padding: 0;
    text-align: left;
  }
  pre {
    margin: 0;
    padding: 6px 8px;
    border-radius: var(--radius);
    background: var(--bg-subtle);
    font: 11.5px/1.45 var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 320px;
    overflow: auto;
  }
  .cmd {
    color: var(--text);
  }
  .out {
    color: var(--text-2);
  }
  .diff {
    border-radius: var(--radius);
    background: var(--bg-subtle);
    font: 11.5px/1.45 var(--mono);
    overflow-x: auto;
  }
  .stat {
    padding: 3px 8px;
    font-size: 11px;
  }
  .add {
    color: var(--diff-add);
  }
  .del {
    color: var(--diff-del);
  }
  .dl {
    padding: 0 8px;
    white-space: pre;
  }
  .dl.add {
    background: var(--diff-add-bg);
    color: var(--text);
  }
  .dl.del {
    background: var(--diff-del-bg);
    color: var(--text);
  }
  .sg {
    display: inline-block;
    width: 14px;
    color: var(--text-3);
  }
  .more {
    padding: 2px 8px;
    font-size: 11px;
  }
  .link {
    align-self: flex-start;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
  .link:disabled {
    color: var(--text-3);
    cursor: default;
  }
</style>
