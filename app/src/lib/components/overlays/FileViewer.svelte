<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import Markdown from '../chat/Markdown.svelte';
  import { app, type FileArg } from '../../state.svelte';
  import { api } from '../../api';
  import { fileKind, mimeOf, saveFile } from '../../files';
  import { t } from '../../i18n.svelte';

  let { file, onclose }: { file: FileArg; onclose: () => void } = $props();

  const MAX_TEXT = 1 << 20;
  const kind = $derived(fileKind(file.path));
  const name = $derived(file.path.slice(file.path.lastIndexOf('/') + 1) || file.path);
  const pdfBlocked = $derived(kind === 'pdf' && app.os === 'android');
  let url = $state<string | null>(null);
  let text = $state<string | null>(null);
  let truncated = $state(false);
  let error = $state<string | null>(null);
  let full = $state(false);
  let saving = $state(false);

  $effect(() => {
    const { path, term, line } = file;
    const k = kind;
    const client = app.client;
    url = text = error = null;
    truncated = full = false;
    if (!client || !k || pdfBlocked) return;
    let alive = true;
    let made: string | null = null;
    api(client)
      .fileBytes({ path, ...(term && { term }), ...(line && { line }) })
      .then(
        (buf) => {
          if (!alive) return;
          if (k === 'markdown' || k === 'text') {
            truncated = buf.byteLength > MAX_TEXT;
            text = new TextDecoder().decode(buf.slice(0, MAX_TEXT));
          } else url = made = URL.createObjectURL(new Blob([buf], { type: mimeOf(path) }));
        },
        (e) => {
          if (alive) error = e instanceof Error ? e.message : String(e);
        },
      );
    return () => {
      alive = false;
      if (made) URL.revokeObjectURL(made);
    };
  });

  function copy() {
    app.client?.copyText(file.path).then(() => app.notify(t('file.copied')), () => {});
  }

  async function download() {
    const client = app.client;
    if (!client || saving) return;
    saving = true;
    try {
      await saveFile(client, file, name).then((path) => app.notify(t('file.saved', { path })));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={name} {onclose} width={960}>
  <div class="file-viewer">
    <p class="path" title={file.path}>{file.path}</p>
    {#if !kind}
      <p class="msg">{t('file.unsupported')}</p>
    {:else if pdfBlocked}
      <p class="msg">{t('file.pdfUnavailable')}</p>
    {:else if error}
      <p class="msg error">{error}</p>
    {:else if url && kind === 'image'}
      <button class="img" class:full onclick={() => (full = !full)}><img src={url} alt={name} /></button>
    {:else if url && kind === 'video'}
      <video src={url} controls preload="metadata" playsinline><track kind="captions" /></video>
    {:else if url && kind === 'audio'}
      <audio src={url} controls></audio>
    {:else if url && kind === 'pdf'}
      <iframe src={url} title={name}></iframe>
    {:else if text !== null && kind === 'markdown'}
      <Markdown {text} />
    {:else if text !== null}
      <pre>{text}</pre>
    {:else}
      <p class="msg">{t('common.loading')}</p>
    {/if}
    {#if truncated}<p class="msg">{t('file.truncated')}</p>{/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={copy}>{t('file.copyPath')}</button>
    <button class="btn primary" onclick={download} disabled={saving || !!error}>{t('file.download')}</button>
  {/snippet}
</Modal>

<style>
  .file-viewer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .path {
    margin: 0;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-3);
    overflow-wrap: anywhere;
  }
  .msg {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .error {
    color: var(--st-failed);
  }
  .img {
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: var(--surface-2);
    border-radius: var(--radius);
    cursor: zoom-in;
    overflow: auto;
  }
  .img img {
    max-width: 100%;
    max-height: 62vh;
    object-fit: contain;
  }
  .img.full {
    place-items: start;
    cursor: zoom-out;
  }
  .img.full img {
    max-width: none;
    max-height: none;
  }
  video {
    width: 100%;
    max-height: 62vh;
    background: #000;
    border-radius: var(--radius);
  }
  audio {
    width: 100%;
  }
  iframe {
    width: 100%;
    height: 62vh;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  pre {
    margin: 0;
    padding: 10px 12px;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.5;
    background: var(--surface-2);
    border-radius: var(--radius);
    overflow: auto;
    white-space: pre;
  }
  :global(.phone .backdrop .modal:has(.file-viewer)) {
    height: 100dvh;
    max-height: 100dvh;
    border-radius: 0;
    padding-top: var(--safe-top);
  }
  :global(.phone .backdrop .modal:has(.file-viewer) > .content) {
    flex: 1;
  }
  :global(.phone) .img img,
  :global(.phone) video {
    max-height: none;
  }
  :global(.phone) iframe {
    height: 70dvh;
  }
</style>
