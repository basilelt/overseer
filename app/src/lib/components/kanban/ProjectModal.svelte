<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { saveSettings, settings } from '../../settings.svelte';
  import { t } from '../../i18n.svelte';
  import { PROJECT_SWATCHES } from './project-colors';

  let { onclose }: { onclose: () => void } = $props();

  let path = $state('');
  let color = $state(PROJECT_SWATCHES[1]);
  let error = $state('');
  let busy = $state(false);
  const trimmed = $derived(path.trim());

  async function submit() {
    if (!trimmed || busy) return;
    busy = true;
    error = '';
    try {
      const p = await app.api.createProject(trimmed);
      saveSettings({ projectColors: { ...settings.projectColors, [p.path]: color } });
      app.addProject(p);
      onclose();
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      busy = false;
    }
  }

  function autofocus(node: HTMLElement) {
    node.focus();
  }
</script>

<Modal title={t('project.new')} {onclose} width={400}>
  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <label>
      <span>{t('project.path')}</span>
      <input
        class="mono"
        use:autofocus
        bind:value={path}
        placeholder="~/src/my-repo"
        spellcheck="false"
        aria-invalid={!!error}
        aria-describedby={error ? 'project-error' : undefined}
        oninput={() => (error = '')}
      />
      {#if error}<em id="project-error" class="err" role="alert">{error}</em>{:else}<small class="faint">{t('project.pathHint')}</small>{/if}
    </label>
    <div class="field">
      <span>{t('project.color')}</span>
      <div class="swatches" role="radiogroup" aria-label={t('project.color')}>
        {#each PROJECT_SWATCHES as c (c)}
          <button type="button" role="radio" aria-checked={color === c} aria-label={c} style:background={c} onclick={() => (color = c)}>
            {#if color === c}<Check size={12} />{/if}
          </button>
        {/each}
      </div>
    </div>
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
    gap: 14px;
  }
  label,
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label > span,
  .field > span {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .err {
    font-style: normal;
    font-size: 12px;
    color: var(--st-failed);
    overflow-wrap: anywhere;
  }
  small {
    font-size: 11.5px;
  }
  input[aria-invalid='true'] {
    border-color: var(--st-failed);
  }
  .swatches {
    display: flex;
    gap: 8px;
  }
  .swatches button {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #fff;
    transition: transform var(--ease);
  }
  .swatches button[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 3px var(--text-3);
  }
</style>
