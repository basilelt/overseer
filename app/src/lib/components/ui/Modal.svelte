<script lang="ts">
  import type { Snippet } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import { t } from '../../i18n.svelte';
  import { onBack } from '../../mobile/back';

  let { title, onclose, width = 520, fill = false, children, footer }: { title: string; onclose: () => void; width?: number; fill?: boolean; children: Snippet; footer?: Snippet } = $props();

  $effect(() => onBack(() => onclose()));

  let modal: HTMLElement;
  const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

  $effect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const taskId = opener?.closest<HTMLElement>('[data-task]')?.dataset.task;
    if (!modal.contains(document.activeElement)) {
      const first = modal.querySelector('.content')?.querySelector<HTMLElement>(FOCUSABLE);
      (first ?? modal).focus({ preventScroll: true });
    }
    return () => {
      if (modal.contains(document.activeElement) || document.activeElement === document.body) {
        const target = opener?.isConnected ? opener : taskId ? document.querySelector<HTMLElement>(`[data-task="${CSS.escape(taskId)}"]`) : null;
        target?.focus({ preventScroll: true });
      }
    };
  });

  function trap(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    const items = [...modal.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
    if (!items.length) return e.preventDefault();
    const first = items[0];
    const last = items[items.length - 1];
    const a = document.activeElement;
    if (e.shiftKey && (a === first || a === modal)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && a === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal" class:fill role="dialog" aria-modal="true" aria-label={title} tabindex="-1" bind:this={modal} onkeydown={trap} style:width="{width}px">
    <header>
      <h2>{title}</h2>
      <button class="icon-btn" aria-label={t('common.close')} onclick={onclose}><X size={16} /></button>
    </header>
    <div class="content" class:fill>{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: grid;
    place-items: start center;
    padding-top: 12vh;
    z-index: var(--z-modal);
  }
  .modal {
    max-width: calc(100vw - 32px);
    max-height: 76vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .modal.fill {
    height: 76vh;
  }
  .modal:focus {
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 12px 8px 16px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .content {
    padding: 4px 16px 16px;
    overflow: auto;
  }
  .content.fill {
    flex: 1 1 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  :global(.phone) .backdrop {
    place-items: end stretch;
    padding-top: 0;
  }
  :global(.phone) .modal {
    width: 100% !important;
    max-width: 100%;
    max-height: 90dvh;
    border-radius: 14px 14px 0 0;
    padding-bottom: var(--safe-bottom);
    animation: sheet-up 180ms cubic-bezier(0.2, 0, 0, 1);
  }
  :global(.phone) .modal.fill {
    height: 90dvh;
  }
  :global(.phone) header {
    position: relative;
    padding: 18px 8px 8px 16px;
  }
  :global(.phone) header::before {
    content: '';
    position: absolute;
    top: 6px;
    left: 50%;
    width: 36px;
    height: 4px;
    margin-left: -18px;
    border-radius: 2px;
    background: var(--border-strong);
  }
  :global(.phone) h2 {
    font-size: 16px;
  }
  :global(.phone) .content {
    padding: 4px 16px 16px;
  }
  :global(.phone) footer {
    flex-wrap: wrap;
  }
  @keyframes sheet-up {
    from {
      transform: translateY(100%);
    }
  }
</style>
