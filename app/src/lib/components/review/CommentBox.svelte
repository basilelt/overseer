<script lang="ts">
  import FileDiff from '@lucide/svelte/icons/file-diff';
  import { onMount } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { keyAction } from '../../review/draft';
  import { insertSuggestion } from '../../review/threads';

  let {
    value = $bindable(''),
    label,
    placeholder,
    lines,
    rows = 3,
    busy = false,
    autofocus = true,
    onsubmit,
    oncancel,
  }: {
    value?: string;
    label: string;
    placeholder?: string;
    lines?: string[];
    rows?: number;
    busy?: boolean;
    autofocus?: boolean;
    onsubmit: () => void;
    oncancel?: () => void;
  } = $props();

  let area: HTMLTextAreaElement | undefined = $state();

  onMount(() => {
    if (autofocus) area?.focus();
  });

  function onKey(e: KeyboardEvent) {
    const a = keyAction(e);
    if (a === 'submit' && value.trim()) {
      e.preventDefault();
      onsubmit();
    } else if (a === 'cancel' && oncancel) {
      e.preventDefault();
      e.stopPropagation();
      oncancel();
    }
  }

  function suggest() {
    if (!lines) return;
    value = insertSuggestion(value, lines);
    area?.focus();
  }
</script>

<div class="box">
  <textarea bind:this={area} bind:value {rows} placeholder={placeholder ?? t('review.placeholder')} onkeydown={onKey} spellcheck="true"></textarea>
  <div class="actions">
    {#if lines}
      <button class="btn ghost small" type="button" title={t('review.suggest')} onclick={suggest}><FileDiff size={13} />{t('review.suggest')}</button>
    {/if}
    <span class="spacer"></span>
    {#if oncancel}<button class="btn small" type="button" onclick={oncancel}>{t('common.cancel')}</button>{/if}
    <button class="btn primary small" type="button" disabled={!value.trim() || busy} onclick={onsubmit} title="Ctrl+Enter">{label}</button>
  </div>
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  textarea {
    width: 100%;
    resize: vertical;
    min-height: 60px;
    font-family: var(--font);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .small {
    height: 26px;
    font-size: 12px;
  }
</style>
