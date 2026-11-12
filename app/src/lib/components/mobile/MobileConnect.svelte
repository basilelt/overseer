<script lang="ts">
  import { onMount } from 'svelte';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import X from '@lucide/svelte/icons/x';
  import Plus from '@lucide/svelte/icons/plus';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Server from '@lucide/svelte/icons/server';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Trash from '@lucide/svelte/icons/trash-2';
  import Unplug from '@lucide/svelte/icons/unplug';
  import Check from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Circle from '@lucide/svelte/icons/circle';
  import Minus from '@lucide/svelte/icons/circle-minus';
  import Loader from '@lucide/svelte/icons/loader-circle';
  import Play from '@lucide/svelte/icons/play';
  import LogOut from '@lucide/svelte/icons/log-out';
  import AuthPrompt from '../overlays/AuthPrompt.svelte';
  import DeviceKey from './DeviceKey.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { isMockClient } from '../../client';
  import { app } from '../../state.svelte';
  import { isBusy, steps } from '../../conn';
  import { exitDemo, isDemo, startDemo } from '../../demo';
  import { connectHint, describeHost, emptyForm, formFromHost, hostFromForm, type FormError, type HostForm } from '../../mobile/hosts';
  import type { ConnPhase, HostTarget, SavedHost } from '../../types';

  let { onclose, initial = null }: { onclose: () => void; initial?: string | null } = $props();

  type Screen = 'list' | 'form' | 'key' | 'progress';

  const resuming = !!app.conn.prompt || (app.conn.target.kind === 'direct' && !!app.conn.target.host && (isBusy(app.conn.phase) || app.conn.phase === 'error'));
  let screen = $state<Screen>(initial === 'devicekey' ? 'key' : resuming ? 'progress' : 'list');
  let back = $state<Screen>('list');
  let hosts = $state<SavedHost[]>([]);
  let form = $state<HostForm>(emptyForm());
  let formError = $state<FormError | null>(null);
  let saveError = $state<string | null>(null);
  let saving = $state(false);
  let armed = $state(false);
  let reached = $state<ConnPhase[]>([]);
  let lastTarget = $state<HostTarget | null>(resuming ? app.conn.target : null);

  onMount(() => {
    load();
  });

  async function load() {
    try {
      hosts = (await app.client?.savedHosts()) ?? [];
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  $effect(() => {
    const p = app.conn.phase;
    if (screen === 'progress' && !reached.includes(p)) reached = [...reached, p];
  });

  $effect(() => {
    if (screen !== 'progress' || app.conn.phase !== 'connected') return;
    const id = setTimeout(onclose, 600);
    return () => clearTimeout(id);
  });

  const stepList = $derived(steps(app.conn, reached).filter((s) => s.id !== 'installing' || reached.includes('installing')));
  const busy = $derived(isBusy(app.conn.phase));
  const hint = $derived(app.conn.phase === 'error' ? connectHint(app.conn.error) : null);
  const current = $derived(app.conn.target.kind === 'direct' ? app.conn.target.host : undefined);
  const editing = $derived(hosts.find((h) => h.id === form.id) ?? null);
  const canClose = $derived(app.connected || isMockClient(app.client));
  const demo = isDemo();
  const canDemo = $derived(!demo && !!app.client && !isMockClient(app.client));

  function go(next: Screen) {
    back = screen === 'form' ? 'list' : screen;
    screen = next;
  }

  function connect(h: SavedHost) {
    if (busy && current === h.id) return go('progress');
    lastTarget = { kind: 'direct', host: h.id, label: h.label };
    reached = [];
    go('progress');
    app.connect(lastTarget);
  }

  function retry() {
    if (!lastTarget) return;
    reached = [];
    app.connect(lastTarget);
  }

  function edit(h: SavedHost | null) {
    form = h ? formFromHost(h) : emptyForm();
    formError = null;
    saveError = null;
    armed = false;
    go('form');
  }

  async function save(andConnect: boolean) {
    const r = hostFromForm(form);
    if ('error' in r) {
      formError = r.error;
      return;
    }
    formError = null;
    saveError = null;
    saving = true;
    try {
      const all = (await app.client?.saveHost(r.host)) ?? [];
      const known = new Set(hosts.map((h) => h.id));
      hosts = all;
      const saved = r.host.id ? all.find((h) => h.id === r.host.id) : all.find((h) => !known.has(h.id));
      if (andConnect && saved) connect(saved);
      else screen = 'list';
    } catch (e) {
      saveError = String((e as Error)?.message ?? e);
    } finally {
      saving = false;
    }
  }

  async function remove(h: SavedHost) {
    if (!armed) {
      armed = true;
      return;
    }
    armed = false;
    try {
      hosts = (await app.client?.deleteHost(h.id)) ?? [];
      screen = 'list';
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async function forget(id: string | undefined) {
    const h = hosts.find((x) => x.id === id);
    if (!h) return;
    try {
      await app.client?.forgetHostKey(h.id);
      app.notify(t('mobile.connect.forgotKey', { name: h.label }));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async function disconnect() {
    await app.disconnect();
  }

  const title = $derived(
    screen === 'key' ? t('mobile.key.title') : screen === 'form' ? t(form.id ? 'mobile.connect.edit' : 'mobile.connect.add') : screen === 'progress' ? (lastTarget?.label ?? t('mobile.connect.title')) : t('mobile.connect.title'),
  );
  const stepLabel = (id: string): string =>
    id === 'downloading' && app.conn.phase === 'downloading' && app.conn.detail ? t('mobile.connect.step.downloading', { platform: app.conn.detail }) : t(`connect.step.${id}` as Key);
  const fieldError = (f: FormError) => (formError === f ? t(`mobile.connect.err.${f}` as Key) : null);
</script>

<div class="screen" role="dialog" aria-modal="true" aria-label={title}>
  <header>
    {#if screen !== 'list'}
      <button class="icon" aria-label={t('common.back')} onclick={() => (screen = screen === 'key' ? back : 'list')}><ArrowLeft size={20} /></button>
    {:else}
      <span class="icon spacer"></span>
    {/if}
    <h2>{title}</h2>
    {#if canClose}
      <button class="icon" aria-label={t('common.close')} onclick={onclose}><X size={20} /></button>
    {:else}
      <span class="icon spacer"></span>
    {/if}
  </header>

  <div class="body">
    {#if screen === 'list'}
      <p class="intro">{t('mobile.connect.intro')}</p>
      {#if !hosts.length}
        <p class="empty">{t('mobile.connect.empty')}</p>
      {/if}
      <ul class="hosts">
        {#each hosts as h (h.id)}
          {@const isCurrent = current === h.id}
          <li class:current={isCurrent}>
            <button class="row" onclick={() => connect(h)}>
              <Server size={18} />
              <span class="text">
                <span class="name">{h.label}</span>
                <span class="detail mono">{describeHost(h)}</span>
              </span>
              {#if isCurrent && app.conn.phase === 'connected'}
                <span class="badge on">{t('mobile.connect.connected')}</span>
              {:else if isCurrent && busy}
                <span class="spin"><Loader size={16} /></span>
              {/if}
            </button>
            <button class="icon" aria-label={t('mobile.connect.edit')} onclick={() => edit(h)}><Pencil size={17} /></button>
          </li>
        {/each}
      </ul>
      <div class="stack">
        <button class="btn primary big" onclick={() => edit(null)}><Plus size={17} />{t('mobile.connect.add')}</button>
        <button class="btn big" onclick={() => go('key')}><KeyRound size={17} />{t('mobile.connect.deviceKey')}</button>
        {#if app.connected && app.conn.target.kind === 'direct'}
          <button class="btn big" onclick={disconnect}><Unplug size={17} />{t('mobile.connect.disconnect')}</button>
        {/if}
      </div>
      {#if demo}
        <div class="demo">
          <p>{t('demo.exitHint')}</p>
          <button class="btn big" onclick={() => exitDemo()}><LogOut size={17} />{t('demo.exit')}</button>
        </div>
      {:else if canDemo}
        <div class="demo">
          <p>{t('demo.tryHint')}</p>
          <button class="btn big" onclick={() => startDemo(app.client)}><Play size={17} />{t('demo.try')}</button>
        </div>
      {/if}
    {:else if screen === 'form'}
      <form
        onsubmit={(e) => {
          e.preventDefault();
          save(true);
        }}
      >
        <label>
          <span>{t('mobile.connect.host')}</span>
          <input bind:value={form.host} placeholder="workstation.lan" autocapitalize="off" autocomplete="off" spellcheck="false" inputmode="url" aria-invalid={formError === 'host'} />
          <small class:err={formError === 'host'}>{fieldError('host') ?? t('mobile.connect.hostHint')}</small>
        </label>
        <div class="pair">
          <label class="grow">
            <span>{t('mobile.connect.user')}</span>
            <input bind:value={form.user} autocapitalize="off" autocomplete="username" spellcheck="false" aria-invalid={formError === 'user'} />
            {#if fieldError('user')}<small class="err">{fieldError('user')}</small>{/if}
          </label>
          <label class="port">
            <span>{t('mobile.connect.port')}</span>
            <input bind:value={form.port} inputmode="numeric" pattern="[0-9]*" aria-invalid={formError === 'port'} />
          </label>
        </div>
        {#if fieldError('port')}<small class="err">{fieldError('port')}</small>{/if}
        <label>
          <span>{t('mobile.connect.label')}</span>
          <input bind:value={form.label} placeholder={form.host || t('mobile.connect.labelHint')} />
        </label>
        <fieldset>
          <legend>{t('mobile.connect.auth')}</legend>
          <div class="seg" role="radiogroup">
            <button type="button" role="radio" aria-checked={form.auth === 'key'} class:on={form.auth === 'key'} onclick={() => (form.auth = 'key')}>{t('mobile.connect.authKey')}</button>
            <button type="button" role="radio" aria-checked={form.auth === 'password'} class:on={form.auth === 'password'} onclick={() => (form.auth = 'password')}>{t('mobile.connect.authPassword')}</button>
          </div>
          {#if form.auth === 'password'}
            <label class="check"><input type="checkbox" bind:checked={form.rememberPassword} /><span>{t('mobile.connect.remember')}</span></label>
          {/if}
        </fieldset>
        <label>
          <span>{t('mobile.connect.jump')}</span>
          <input bind:value={form.jump} placeholder="user@bastion:22" autocapitalize="off" autocomplete="off" spellcheck="false" aria-invalid={formError === 'jump'} />
          <small class:err={formError === 'jump'}>{fieldError('jump') ?? t('mobile.connect.jumpHint')}</small>
        </label>
        {#if saveError}<p class="error" role="alert">{saveError}</p>{/if}
        <div class="stack">
          <button type="submit" class="btn primary big" disabled={saving}>{t('mobile.connect.saveConnect')}</button>
          <button type="button" class="btn big" disabled={saving} onclick={() => save(false)}>{t('mobile.connect.save')}</button>
          {#if editing}
            <button type="button" class="btn big" onclick={() => forget(editing.id)}><KeyRound size={17} />{t('mobile.connect.forgetKey')}</button>
            <button type="button" class="btn danger big" onclick={() => remove(editing)}><Trash size={17} />{armed ? t('mobile.connect.deleteConfirm', { name: editing.label }) : t('mobile.connect.delete')}</button>
          {/if}
        </div>
      </form>
    {:else if screen === 'key'}
      <DeviceKey />
    {:else}
      {#if app.conn.resolved}<p class="resolved mono">{app.conn.resolved}</p>{/if}
      <ol class="steps">
        {#each stepList as s (s.id)}
          <li class={s.status}>
            {#if s.status === 'done'}<Check size={16} />{:else if s.status === 'active'}<span class="spin"><Loader size={16} /></span>{:else if s.status === 'failed'}<CircleX size={16} />{:else if s.status === 'skipped'}<Minus size={16} />{:else}<Circle size={16} />{/if}
            <span>{stepLabel(s.id)}</span>
            {#if s.status === 'active' && app.conn.phase === 'reconnecting'}<span class="detail">{t('conn.reconnecting', { n: app.conn.attempt ?? 1 })}</span>{/if}
          </li>
        {/each}
      </ol>
      {#if app.conn.prompt}
        {#key app.conn.prompt.id}<div class="prompt"><AuthPrompt prompt={app.conn.prompt} /></div>{/key}
      {/if}
      {#if app.conn.phase === 'error'}
        <div class="failure" role="alert">
          <strong>{t('conn.error')}</strong>
          {#if hint}<p class="hint">{t(`mobile.connect.hint.${hint}` as Key)}</p>{/if}
          <pre>{app.conn.error}</pre>
        </div>
        <div class="stack">
          <button class="btn primary big" onclick={retry}>{t('common.retry')}</button>
          {#if hint === 'deviceKey'}
            <button class="btn big" onclick={() => go('key')}><KeyRound size={17} />{t('mobile.connect.showKey')}</button>
          {/if}
          {#if hint === 'hostKeyChanged'}
            <button class="btn big" onclick={() => forget(lastTarget?.host)}><KeyRound size={17} />{t('mobile.connect.forgetKey')}</button>
          {/if}
          <button class="btn big" onclick={() => (screen = 'list')}>{t('common.back')}</button>
          {#if canDemo}
            <button class="btn big" onclick={() => startDemo(app.client)}><Play size={17} />{t('demo.try')}</button>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .screen {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    padding: env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px) 0 env(safe-area-inset-left, 0px);
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 52px;
    padding: 0 6px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: var(--radius);
    color: var(--text-2);
  }
  .icon:active {
    background: var(--surface-active);
  }
  .spacer {
    pointer-events: none;
  }
  .body {
    flex: 1;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
    padding: 16px 16px calc(24px + env(safe-area-inset-bottom, 0px));
  }
  .intro,
  .empty {
    margin: 0 0 14px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .empty {
    color: var(--text-3);
  }
  .hosts {
    list-style: none;
    margin: 0 0 16px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .hosts:empty {
    display: none;
  }
  .hosts li {
    display: flex;
    align-items: center;
    padding-right: 4px;
    background: var(--surface);
  }
  .hosts li + li {
    border-top: 1px solid var(--border);
  }
  .hosts li.current {
    background: var(--accent-soft);
  }
  .row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 60px;
    padding: 8px 12px;
    text-align: left;
    color: var(--text-2);
  }
  .row:active {
    background: var(--surface-active);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    color: var(--text);
    font-weight: 500;
    font-size: 15px;
  }
  .detail {
    font-size: 12px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    flex: none;
    font-size: 12px;
    color: var(--st-input);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 16px;
  }
  .demo {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 22px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }
  .demo p {
    margin: 0;
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.5;
  }
  .big {
    min-height: 46px;
    justify-content: center;
    gap: 8px;
    font-size: 15px;
    white-space: normal;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label > span,
  legend {
    font-size: 13px;
    color: var(--text-2);
  }
  input:not([type='checkbox']) {
    height: 46px;
    width: 100%;
    font-size: 16px;
    padding: 0 12px;
  }
  small {
    font-size: 12px;
    color: var(--text-3);
  }
  small.err,
  .err {
    color: var(--st-failed);
  }
  .pair {
    display: flex;
    gap: 10px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .port {
    width: 96px;
  }
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  legend {
    padding: 0;
    margin-bottom: 6px;
  }
  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .seg button {
    flex: 1;
    min-height: 44px;
    color: var(--text-2);
    background: var(--surface);
  }
  .seg button + button {
    border-left: 1px solid var(--border);
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 10px;
    min-height: 44px;
  }
  .check input {
    width: 20px;
    height: 20px;
  }
  .resolved {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--text-3);
    word-break: break-all;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 36px;
    color: var(--text-3);
  }
  .steps li.done {
    color: var(--st-input);
  }
  .steps li.active {
    color: var(--text);
  }
  .steps li.failed {
    color: var(--st-failed);
  }
  .steps li span:not(.spin) {
    color: inherit;
  }
  .prompt :global(input) {
    height: 46px;
    font-size: 16px;
  }
  .prompt :global(.btn) {
    min-height: 44px;
    flex: 1;
    justify-content: center;
  }
  .failure {
    margin-top: 16px;
    padding: 12px;
    border: 1px solid var(--border);
    border-left: 2px solid var(--st-failed);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .failure strong {
    color: var(--st-failed);
    font-weight: 600;
  }
  .hint {
    margin: 8px 0 0;
    color: var(--text);
    line-height: 1.5;
  }
  pre {
    margin: 8px 0 0;
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-2);
    user-select: text;
    -webkit-user-select: text;
  }
  .error {
    color: var(--st-failed);
    margin: 0;
  }
  .spin {
    display: inline-grid;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
