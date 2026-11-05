<script lang="ts">
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { REPO, TOKEN_URL, apk, at, errorText } from '../../mobile/apk-update.svelte';

  let draft = $state('');

  function create() {
    apk.backend?.openUrl(TOKEN_URL).catch((e) => app.notify(String(e?.message ?? e), 'error'));
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (await apk.saveToken(draft)) draft = '';
  }

  async function remove() {
    await apk.removeToken();
    apk.closeToken();
  }
</script>

<Modal title={at('tokenTitle')} onclose={() => apk.closeToken()} width={460}>
  <p class="intro">{at(apk.updates ? 'tokenIntro' : 'tokenIntroServer', { repo: REPO })}</p>
  <ul class="how">
    <li>{at('tokenFine')}</li>
    <li>{at('tokenClassic')}</li>
  </ul>
  <button class="btn link" type="button" onclick={create}><ExternalLink size={15} />{at('tokenCreate')}</button>
  <form id="gh-token" onsubmit={save}>
    <label for="gh-token-input">{at('tokenLabel')}</label>
    <input
      id="gh-token-input"
      class="mono"
      type="password"
      bind:value={draft}
      placeholder="github_pat_…"
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      disabled={apk.tokenBusy}
    />
    <small>{at('tokenStored')}</small>
  </form>
  {#if apk.tokenError}<p class="error" role="alert">{errorText(apk.tokenError)}</p>{/if}

  {#snippet footer()}
    {#if apk.hasToken}<button class="btn ghost danger" type="button" onclick={remove}>{at('tokenRemove')}</button>{/if}
    <span class="grow"></span>
    <button class="btn ghost" type="button" onclick={() => apk.closeToken()}>{at('later')}</button>
    <button class="btn primary" type="submit" form="gh-token" disabled={!draft.trim() || apk.tokenBusy}>{apk.tokenBusy ? at('tokenChecking') : at('tokenSave')}</button>
  {/snippet}
</Modal>

<style>
  .intro {
    margin: 0 0 8px;
    color: var(--text);
    line-height: 1.5;
  }
  .how {
    margin: 0 0 12px;
    padding-left: 18px;
    color: var(--text-2);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .how li {
    margin: 2px 0;
  }
  .link {
    gap: 6px;
    min-height: 40px;
    width: 100%;
    justify-content: center;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 14px;
  }
  label {
    font-size: 12px;
    color: var(--text-3);
  }
  input {
    min-height: 40px;
    min-width: 0;
  }
  small {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .error {
    margin: 10px 0 0;
    color: var(--st-failed);
    line-height: 1.5;
    word-break: break-word;
  }
  .grow {
    flex: 1;
  }
</style>
