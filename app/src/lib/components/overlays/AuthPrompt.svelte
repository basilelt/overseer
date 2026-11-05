<script lang="ts">
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { ConnPrompt } from '../../types';

  let { prompt }: { prompt: ConnPrompt } = $props();

  let secret = $state('');
  let sending = $state(false);

  async function answer(value: string | null) {
    if (sending) return;
    const id = prompt.id;
    sending = true;
    try {
      await app.client?.answerPrompt(id, value);
      if (app.conn.prompt?.id === id) app.conn = { ...app.conn, prompt: undefined };
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      sending = false;
      secret = '';
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopImmediatePropagation();
    answer(null);
  }

  const focus = (el: HTMLElement) => el.focus();
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="prompt" class:warn={prompt.kind === 'hostkey'} role="alertdialog" aria-labelledby="auth-prompt-title">
  {#if prompt.kind === 'hostkey'}
    <div class="head" id="auth-prompt-title"><ShieldAlert size={15} /><strong>{t('auth.hostkey.title')}</strong></div>
    {#if prompt.host}<p class="host mono">{prompt.host}</p>{/if}
    <p>{t('auth.hostkey.warning')}</p>
    {#if prompt.fingerprint}
      <div class="fp">
        <span>{t('auth.hostkey.fingerprint')}</span>
        <code class="mono">{prompt.fingerprint}</code>
      </div>
    {/if}
    <details>
      <summary>{t('auth.details')}</summary>
      <pre>{prompt.text}</pre>
    </details>
    <div class="actions">
      <button class="btn" disabled={sending} onclick={() => answer(null)}>{t('common.cancel')}</button>
      <button class="btn primary" use:focus disabled={sending} onclick={() => answer('yes')}>{t('auth.hostkey.trust')}</button>
    </div>
  {:else if prompt.kind === 'secret'}
    <div class="head" id="auth-prompt-title"><KeyRound size={15} /><strong>{t('auth.secret.title')}</strong></div>
    <pre>{prompt.text}</pre>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        answer(secret);
      }}
    >
      <input type="password" use:focus bind:value={secret} autocomplete="off" aria-label={prompt.text} />
      <div class="actions">
        <button type="button" class="btn" disabled={sending} onclick={() => answer(null)}>{t('common.cancel')}</button>
        <button type="submit" class="btn primary" disabled={sending}>{t('auth.submit')}</button>
      </div>
    </form>
  {:else}
    <div class="head" id="auth-prompt-title"><KeyRound size={15} /><strong>{t('auth.confirm.title')}</strong></div>
    <pre>{prompt.text}</pre>
    <div class="actions">
      <button class="btn" disabled={sending} onclick={() => answer(null)}>{t('common.cancel')}</button>
      <button class="btn primary" use:focus disabled={sending} onclick={() => answer('yes')}>{t('auth.confirm.yes')}</button>
    </div>
  {/if}
</div>

<style>
  .prompt {
    margin-top: 14px;
    padding: 12px;
    border: 1px solid var(--border);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .prompt.warn {
    border-left-color: var(--st-failed);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .warn .head {
    color: var(--st-failed);
  }
  strong {
    font-weight: 600;
  }
  p {
    margin: 8px 0 0;
    color: var(--text-2);
  }
  .host {
    color: var(--text);
    user-select: text;
    -webkit-user-select: text;
  }
  .fp {
    display: grid;
    gap: 4px;
    margin-top: 10px;
    font-size: 11px;
    color: var(--text-3);
  }
  code {
    font-size: 13px;
    color: var(--text);
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    word-break: break-all;
    user-select: text;
    -webkit-user-select: text;
  }
  details {
    margin-top: 10px;
    font-size: 12px;
    color: var(--text-3);
  }
  summary {
    cursor: pointer;
  }
  pre {
    margin: 6px 0 0;
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-2);
    user-select: text;
    -webkit-user-select: text;
  }
  input {
    width: 100%;
    margin-top: 10px;
    height: 30px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
