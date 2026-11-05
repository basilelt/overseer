<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { byRank } from '../../workspaces';
  import { baseName, tildePath, type DirListing } from '../../dirs';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FolderBrowser from './FolderBrowser.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let path = $state('');
  let name = $state('');
  let folderId = $state<string>(app.currentWorkspace?.folderId ?? '');
  let pinned = $state(false);
  let error = $state('');
  let busy = $state(false);
  let browsing = $state(false);
  let browser = $state<ReturnType<typeof FolderBrowser>>();
  const trimmed = $derived(path.trim());
  const folders = $derived(
    app.ws.folders
      .filter((f) => !f.parentId)
      .sort(byRank)
      .flatMap((f) => [f, ...app.ws.folders.filter((c) => c.parentId === f.id).sort(byRank)]),
  );
  const suggestion = $derived(trimmed.replace(/\/+$/, '').split('/').pop() ?? '');

  async function submit() {
    if (!trimmed || busy) return;
    busy = true;
    error = '';
    try {
      const w = await app.api.createWorkspace({ path: trimmed, name: name.trim() || undefined, folderId: folderId || null, pinned });
      app.upsertWorkspace(w);
      app.selectWorkspace(w.id);
      onclose();
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      busy = false;
    }
  }

  function browse() {
    if (!browsing) browsing = true;
    else if (trimmed) browser?.jump(trimmed);
    else browsing = false;
  }

  function pick(l: DirListing) {
    path = tildePath(l.path, l.home);
    if (!name.trim()) name = baseName(l.path);
    error = '';
    browsing = false;
  }

  function autofocus(node: HTMLElement) {
    node.focus();
  }
</script>

<Modal title={t('ws.new')} {onclose} width={420}>
  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <div class="field">
      <label for="ws-path">{t('ws.folderPath')}</label>
      <div class="path">
        <input id="ws-path" class="mono" use:autofocus bind:value={path} placeholder="~/src/my-project" spellcheck="false" aria-invalid={!!error} oninput={() => (error = '')} />
        <button type="button" class="btn" aria-expanded={browsing} onclick={browse}><FolderOpen size={14} />{t('ws.browse')}</button>
      </div>
      {#if error}<em class="err" role="alert">{error}</em>{:else}<small class="faint">{t('ws.folderHint')}</small>{/if}
      {#if browsing}<FolderBrowser bind:this={browser} start={trimmed} onpick={pick} />{/if}
    </div>
    <label>
      <span>{t('ws.name')}</span>
      <input bind:value={name} placeholder={suggestion} maxlength="80" />
    </label>
    <label>
      <span>{t('ws.folder')}</span>
      <select bind:value={folderId}>
        <option value="">{t('ws.topLevel')}</option>
        {#each folders as f (f.id)}<option value={f.id}>{f.parentId ? `— ${f.name}` : f.name}</option>{/each}
      </select>
    </label>
    <label class="check"><input type="checkbox" bind:checked={pinned} />{t('ws.pin')}</label>
  </form>
  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn primary" disabled={!trimmed || busy} onclick={submit}>{t('common.create')}</button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  label,
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .path {
    display: flex;
    gap: 6px;
  }
  .path input {
    flex: 1;
    min-width: 0;
  }
  .field > label,
  label > span {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }
  small {
    font-size: 11.5px;
  }
  .err {
    font-style: normal;
    font-size: 12px;
    color: var(--st-failed);
  }
</style>
