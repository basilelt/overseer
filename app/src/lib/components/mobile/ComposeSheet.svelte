<script lang="ts" module>
  const drafts = new Map<string, string>();
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { composeOutcome, composeRequest, composeTarget, type ComposeMode } from '../../mobile/compose';
  import { app } from '../../state.svelte';
  import Modal from '../ui/Modal.svelte';
  import { isNativeAndroid, native } from '../../mobile/native';
  import { insertText } from '../../uploads';
  import { attachImages } from '../../mobile/compose-attach';
  import { isSubmitChord } from '../../chord';

  let { targetKey, onclose }: { targetKey: string | null; onclose: () => void } = $props();

  const key = $derived(targetKey ?? '');
  let text = $state(untrack(() => drafts.get(targetKey ?? '') ?? ''));
  let sending = $state(false);
  const target = $derived(composeTarget(targetKey));

  $effect(() => {
    drafts.set(key, text);
  });

  async function send(mode: ComposeMode) {
    const req = composeRequest(text, target, mode);
    if (!req) {
      if (!target) app.notify(t('mobile.compose.noTarget'), 'error');
      return;
    }
    sending = true;
    try {
      const res = await app.api.broadcast(req);
      const out = composeOutcome(res.results);
      if (out.kind === 'failed') return app.notify(t('mobile.compose.failed', { reason: out.reason }), 'error');
      app.notify(t(out.kind === 'sent' ? 'mobile.compose.sent' : 'mobile.compose.queued'));
      drafts.delete(key);
      text = '';
      onclose();
    } catch (e) {
      app.notify(t('mobile.compose.failed', { reason: String((e as Error)?.message ?? e) }), 'error');
    } finally {
      sending = false;
    }
  }

  async function onpaste(e: ClipboardEvent) {
    const dt = e.clipboardData;
    const files = Array.from(dt?.files ?? []).filter((f) => f.type.startsWith('image/'));
    if (!files.length && (dt?.getData('text/plain') || !isNativeAndroid())) return;
    e.preventDefault();
    const el = e.currentTarget as HTMLTextAreaElement;
    const [from, to] = [el.selectionStart, el.selectionEnd];
    sending = true;
    try {
      const paths = await attachImages(files, { client: app.requireClient(), clipboardImage: native.clipboardImage, notify: (...a) => app.notify(...a) });
      if (paths.length) text = text.slice(0, from) + insertText(paths) + text.slice(to);
      else if (!files.length) app.notify(t('mobile.term.pasteFailed'));
    } catch (err) {
      app.notify(t('upload.pasteFailed', { name: t('mobile.term.clipboard'), error: String((err as Error)?.message ?? err) }), 'error');
    } finally {
      sending = false;
    }
  }

  function focus(el: HTMLTextAreaElement) {
    el.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (isSubmitChord(e)) {
      e.preventDefault();
      void send('queue');
    }
  }
</script>

<Modal title={t('mobile.compose.title')} {onclose}>
  <textarea use:focus bind:value={text} {onkeydown} {onpaste} rows="6" placeholder={t('mobile.compose.placeholder')} disabled={sending} spellcheck="true"></textarea>
  {#snippet footer()}
    <button class="btn" disabled={sending || !text.trim()} onclick={() => send('now')}>{t('mobile.compose.now')}</button>
    <button class="btn primary" disabled={sending || !text.trim()} onclick={() => send('queue')}>{t('mobile.compose.queue')}</button>
  {/snippet}
</Modal>

<style>
  textarea {
    display: block;
    width: 100%;
    min-height: 140px;
    box-sizing: border-box;
    resize: vertical;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--text);
    font: 15px/1.45 var(--font);
  }
  textarea:focus {
    outline: 2px solid var(--focus);
    outline-offset: -1px;
  }
  :global(.phone) .btn {
    min-height: 44px;
  }
</style>
