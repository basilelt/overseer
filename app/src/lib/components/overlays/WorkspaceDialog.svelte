<script lang="ts">
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { byRank } from '../../workspaces';
  import { baseName, tildePath, type DirListing } from '../../dirs';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FolderBrowser from './FolderBrowser.svelte';
  import { relativeTo, subjectBody } from '../../subject-form';

  let { onclose }: { onclose: () => void } = $props();

  // Opened with a workspace id: a subject (sub-folder, submodule or path-less) nested in that workspace.
  const parent = app.overlayArg ? app.ws.workspaces.find((w) => w.id === app.overlayArg && !w.parentId && !!w.path) : undefined;
  let startAtRoot = $state(true);

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

  const subject = $derived(parent ? subjectBody(parent, path, name, startAtRoot) : null);
  const ready = $derived(parent ? !!subject : !!trimmed);

  async function submit() {
    if (!ready || busy) return;
    busy = true;
    error = '';
    try {
      const w = await app.api.createWorkspace(subject ?? { path: trimmed, name: name.trim() || undefined, folderId: folderId || null, pinned });
      app.upsertWorkspace(w);
      app.selectWorkspace(w.id);
      onclose();
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      busy = false;
    }
  }

  const browseFrom = $derived(parent && !trimmed.startsWith('/') && !trimmed.startsWith('~') ? `${parent.path}${trimmed ? `/${trimmed}` : ''}` : trimmed);

  function browse() {
    if (!browsing) browsing = true;
    else if (trimmed) browser?.jump(browseFrom);
    else browsing = false;
  }

  function pick(l: DirListing) {
    path = parent ? relativeTo(parent, l.path) : tildePath(l.path, l.home);
    if (!name.trim()) name = baseName(l.path);
    error = '';
    browsing = false;
  }

  function autofocus(node: HTMLElement) {
    node.focus();
  }
</script>

<Modal title={parent ? t('ws.sub.newIn', { name: parent.name }) : t('ws.new')} {onclose} width={420}>
  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <div class="field">
      <label for="ws-path">{parent ? t('ws.sub.path', { name: parent.name }) : t('ws.folderPath')}</label>
      <div class="path">
        <input id="ws-path" class="mono" use:autofocus bind:value={path} placeholder={parent ? '20_processus/onboarding' : '~/src/my-project'} spellcheck="false" aria-invalid={!!error} oninput={() => (error = '')} />
        <button type="button" class="btn" aria-expanded={browsing} onclick={browse}><FolderOpen size={14} />{t('ws.browse')}</button>
      </div>
      {#if error}<em class="err" role="alert">{error}</em>{:else}<small class="faint">{parent ? t('ws.sub.pathHint') : t('ws.folderHint')}</small>{/if}
      {#if browsing}<FolderBrowser bind:this={browser} start={browseFrom} onpick={pick} />{/if}
    </div>
    <label>
      <span>{t('ws.name')}</span>
      <input bind:value={name} placeholder={suggestion} maxlength="80" />
    </label>
    {#if parent}
      {#if trimmed}
        <label>
          <span>{t('ws.sub.startIn')}</span>
          <select bind:value={startAtRoot}>
            <option value={true}>{t('ws.sub.startRoot', { path: parent.path ?? '' })}</option>
            <option value={false}>{t('ws.sub.startOwn')}</option>
          </select>
        </label>
      {/if}
      <small class="faint">{t('ws.sub.submodulesHint')}</small>
    {:else}
      <label>
        <span>{t('ws.folder')}</span>
        <select bind:value={folderId}>
          <option value="">{t('ws.topLevel')}</option>
          {#each folders as f (f.id)}<option value={f.id}>{f.parentId ? `— ${f.name}` : f.name}</option>{/each}
        </select>
      </label>
      <label class="check"><input type="checkbox" bind:checked={pinned} />{t('ws.pin')}</label>
    {/if}
  </form>
  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn primary" disabled={!ready || busy} onclick={submit}>{t('common.create')}</button>
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
