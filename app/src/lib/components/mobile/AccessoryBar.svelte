<script lang="ts">
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste';
  import MessageSquarePlus from '@lucide/svelte/icons/message-square-plus';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import { t } from '../../i18n.svelte';
  import { ACCESSORY_KEYS, LABELS, pressKey, type AccessoryKey, type Mods } from '../../mobile/accessory';

  let {
    mods = $bindable(),
    onsend,
    onpaste,
    onupload,
    oncompose,
  }: { mods: Mods; onsend: (data: string) => void; onpaste: () => void; onupload: () => void; oncompose: () => void } = $props();

  const DOUBLE_TAP_MS = 320;
  let lastTap = { key: '', at: 0 };

  function tap(key: AccessoryKey) {
    const now = Date.now();
    const double = lastTap.key === key && now - lastTap.at < DOUBLE_TAP_MS;
    lastTap = { key, at: now };
    const p = pressKey(key, mods, double);
    mods = p.mods;
    if (p.send) onsend(p.send);
  }

  function keep(e: PointerEvent) {
    e.preventDefault();
  }

  const armed = (k: AccessoryKey) => (k === 'ctrl' ? mods.ctrl : k === 'alt' ? mods.alt : 'off');
</script>

<div class="bar" role="toolbar" aria-label={t('mobile.term.keys')}>
  <div class="keys">
    {#each ACCESSORY_KEYS as k (k)}
      <button
        type="button"
        class="key"
        class:once={armed(k) === 'once'}
        class:locked={armed(k) === 'locked'}
        aria-pressed={k === 'ctrl' || k === 'alt' ? armed(k) !== 'off' : undefined}
        onpointerdown={keep}
        onclick={() => tap(k)}>{LABELS[k]}</button
      >
    {/each}
  </div>
  <button type="button" class="key icon" aria-label={t('mobile.term.paste')} onpointerdown={keep} onclick={onpaste}><ClipboardPaste size={17} /></button>
  <button type="button" class="key icon" aria-label={t('mobile.term.upload')} onpointerdown={keep} onclick={onupload}><Paperclip size={17} /></button>
  <button type="button" class="key icon" aria-label={t('mobile.term.compose')} onpointerdown={keep} onclick={oncompose}><MessageSquarePlus size={17} /></button>
</div>

<style>
  .bar {
    flex: none;
    display: flex;
    align-items: stretch;
    gap: 4px;
    padding: 4px 6px;
    background: var(--surface-2);
    border-top: 1px solid var(--border);
  }
  .keys {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 4px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .keys::-webkit-scrollbar {
    display: none;
  }
  .key {
    flex: none;
    min-width: 44px;
    height: 44px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: 500 14px var(--mono);
    touch-action: manipulation;
    -webkit-user-select: none;
    user-select: none;
  }
  .key:active {
    background: var(--surface-active);
  }
  .icon {
    display: grid;
    place-items: center;
    padding: 0;
  }
  .once {
    border-color: var(--accent);
    color: var(--accent);
  }
  .locked {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
</style>
