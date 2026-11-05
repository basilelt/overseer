<script lang="ts">
  import { onMount } from 'svelte';
  import Copy from '@lucide/svelte/icons/copy';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { DeviceKey } from '../../types';

  let key = $state<DeviceKey | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    app.client
      ?.deviceKey()
      .then((k) => (key = k))
      .catch((e) => (error = String((e as Error)?.message ?? e)));
  });

  async function copy(text: string) {
    try {
      await app.client?.copyText(text);
      app.notify(t('mobile.key.copied'));
    } catch (e) {
      app.notify(t('mobile.key.copyFailed', { error: String((e as Error)?.message ?? e) }), 'error');
    }
  }
</script>

<div class="key">
  <p class="intro">{t('mobile.key.intro')}</p>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !key}
    <p class="faint">{t('mobile.key.loading')}</p>
  {:else}
    <span class="label">{t('mobile.key.public')}</span>
    <code class="mono public">{key.publicKey}</code>
    <span class="label">{t('mobile.key.fingerprint')}</span>
    <code class="mono">{key.fingerprint}</code>
    <div class="actions">
      <button class="btn primary" onclick={() => copy(key!.installCommand)}><Copy size={15} />{t('mobile.key.copyCommand')}</button>
      <button class="btn" onclick={() => copy(key!.publicKey)}><Copy size={15} />{t('mobile.key.copyPublic')}</button>
    </div>
    <h3>{t('mobile.key.howTitle')}</h3>
    <ol>
      <li>{t('mobile.key.how1')}</li>
      <li>{t('mobile.key.how2')}</li>
      <li>{t('mobile.key.how3')}</li>
    </ol>
  {/if}
</div>

<style>
  .key {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .intro {
    margin: 0 0 8px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .label {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-3);
  }
  code {
    display: block;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    color: var(--text);
    font-size: 12px;
    word-break: break-all;
    user-select: text;
    -webkit-user-select: text;
  }
  .public {
    line-height: 1.5;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
  }
  .actions .btn {
    min-height: 44px;
    justify-content: center;
    gap: 8px;
  }
  h3 {
    margin: 18px 0 0;
    font-size: 13px;
    font-weight: 600;
  }
  ol {
    margin: 0;
    padding-left: 20px;
    color: var(--text-2);
    line-height: 1.5;
  }
  li + li {
    margin-top: 6px;
  }
  .error {
    color: var(--st-failed);
  }
</style>
