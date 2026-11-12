<script lang="ts">
  import { onMount } from 'svelte';
  import Monitor from '@lucide/svelte/icons/monitor';
  import Server from '@lucide/svelte/icons/server';
  import Clock from '@lucide/svelte/icons/clock';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import Check from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Circle from '@lucide/svelte/icons/circle';
  import Minus from '@lucide/svelte/icons/circle-minus';
  import Loader from '@lucide/svelte/icons/loader-circle';
  import Modal from '../ui/Modal.svelte';
  import AuthPrompt from './AuthPrompt.svelte';
  import LocalNetworkPanel from './LocalNetworkPanel.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { saveSettings, settings } from '../../settings.svelte';
  import { connectHow, preferredChoice, type ConnectHow, type HostChoice } from '../../hosts';
  import { hostKey, openWindow } from '../../window';
  import { isBusy, steps } from '../../conn';
  import { exitDemo, isDemo, startDemo } from '../../demo';
  import { localNetworkView } from '../../localnet';
  import { fuzzy } from '../../fuzzy';
  import type { ConnPhase, HostTarget, SshHost } from '../../types';

  let { onclose }: { onclose: () => void } = $props();

  const demo = isDemo();
  const canDemo = $derived(!demo && !!app.client && app.client.kind !== 'mock');

  interface Row {
    key: string;
    target: HostTarget;
    detail?: string;
    section: 'local' | 'recent' | 'config' | 'typed';
  }

  let query = $state('');
  let active = $state(0);
  let hosts = $state<SshHost[]>([]);
  const resuming = !!app.conn.prompt;
  let started = $state(resuming);
  let reached = $state<ConnPhase[]>([]);
  let lastTarget = $state<HostTarget | null>(resuming ? app.conn.target : null);

  onMount(() => {
    app.client
      ?.sshHosts()
      .then((h) => (hosts = h))
      .catch(() => {});
  });

  const rows = $derived.by<Row[]>(() => {
    const q = query.trim();
    const ok = (s: string) => !q || fuzzy(q, s) !== null;
    const recent = settings.recentHosts.filter((h) => h.kind === 'ssh');
    const seen = new Set(recent.map((h) => h.host));
    const out: Row[] = [];
    if (q && !recent.some((h) => h.host === q) && !hosts.some((h) => h.alias === q)) {
      out.push({ key: `typed:${q}`, target: { kind: 'ssh', host: q, label: q }, section: 'typed' });
    }
    if (ok(t('connect.local'))) out.push({ key: 'local', target: { kind: 'local', label: t('host.local') }, section: 'local' });
    for (const h of recent) if (ok(h.host ?? '')) out.push({ key: `recent:${h.host}`, target: h, section: 'recent' });
    for (const h of hosts) {
      if (seen.has(h.alias) || !ok(`${h.alias} ${h.hostName ?? ''}`)) continue;
      const detail = [h.user && `${h.user}@`, h.hostName ?? h.alias, h.port && h.port !== 22 ? `:${h.port}` : ''].filter(Boolean).join('');
      out.push({ key: `config:${h.alias}`, target: { kind: 'ssh', host: h.alias, label: h.alias }, detail, section: 'config' });
    }
    return out;
  });

  $effect(() => {
    query;
    active = 0;
  });

  // Adding a host swaps the main area to it after a tick: until then app.conn is still the previous host.
  const mine = $derived(!!lastTarget && hostKey(app.conn.target) === hostKey(lastTarget));

  $effect(() => {
    const p = app.conn.phase;
    if (started && mine && !reached.includes(p)) reached = [...reached, p];
  });

  $effect(() => {
    if (!started || !mine || app.conn.phase !== 'connected') return;
    const id = setTimeout(onclose, 600);
    return () => clearTimeout(id);
  });

  const stepList = $derived(steps(app.conn, reached));
  const busy = $derived(started && isBusy(app.conn.phase));
  const lnView = $derived(app.conn.phase === 'error' ? localNetworkView(app.localNet, app.conn.error) : 'none');
  const lnReport = $derived(lnView === 'none' ? null : app.localNet);

  // Opened from "Add a host to this window": the target joins the hosts already here.
  const forced: HostChoice | undefined = app.overlayArg === 'add' ? 'add' : undefined;
  let asking = $state<HostTarget | null>(askedFor(app.overlayArg));
  let remember = $state(false);
  const first = $derived(preferredChoice(settings));
  const order = $derived<HostChoice[]>(first === 'add' ? ['window', 'add'] : ['add', 'window']);

  function askedFor(arg: string | null): HostTarget | null {
    if (!arg?.startsWith('ask:')) return null;
    try {
      const h = JSON.parse(arg.slice(4)) as HostTarget;
      return h && (h.kind === 'local' || (h.kind === 'ssh' && h.host)) ? h : null;
    } catch {
      return null;
    }
  }

  function connect(target: HostTarget | undefined) {
    if (!target || busy) return;
    const how = connectHow(settings, target, { active: hostKey(app.conn.target), keys: app.hosts.map((l) => l.key), live: app.connected }, forced);
    if (how === 'ask') {
      asking = target;
      remember = false;
      return;
    }
    go(target, how);
  }

  function go(target: HostTarget, how: Exclude<ConnectHow, 'ask'>) {
    if (how === 'window') {
      void openWindow(target);
      onclose();
      return;
    }
    if (how === 'switch') {
      void app.switchHost(hostKey(target));
      onclose();
      return;
    }
    lastTarget = target;
    reached = [];
    started = true;
    app.connect(target, how === 'add' ? 'add' : 'replace');
  }

  function choose(c: HostChoice) {
    const target = asking;
    if (!target) return;
    if (remember) saveSettings({ hostChoice: { ...settings.hostChoice, [hostKey(target)]: c } });
    asking = null;
    go(target, c);
  }

  function onkeydown(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === 'ArrowDown') active = n ? (active + 1) % n : 0;
    else if (e.key === 'ArrowUp') active = n ? (active - 1 + n) % n : 0;
    else if (e.key === 'Enter') connect(rows[active]?.target);
    else return;
    e.preventDefault();
  }

  const sectionLabel: Record<Row['section'], Key | null> = { typed: null, local: null, recent: 'connect.recent', config: 'connect.sshConfig' };
  const stepLabel = (id: string): string =>
    id === 'installing' && app.conn.version ? t('conn.installing', { version: app.conn.version }) : t(`connect.step.${id}` as Key);
  const focus = (el: HTMLInputElement) => el.focus();
  const autofocus = (el: HTMLButtonElement, on: boolean) => {
    if (on) el.focus();
  };
</script>

<Modal title={t('connect.title')} {onclose} width={540}>
  {#if asking}
    {@const name = asking.kind === 'local' ? t('connect.local') : asking.label}
    <div class="choose">
      <div class="target">
        {#if asking.kind === 'local'}<Monitor size={15} />{:else}<Server size={15} />{/if}
        <span>{t('connect.choose', { host: name })}</span>
      </div>
      <label class="remember"><input type="checkbox" bind:checked={remember} />{t('connect.remember', { host: name })}</label>
      <div class="actions">
        <button class="btn" onclick={() => (asking = null)}>{t('common.back')}</button>
        {#each order as c (c)}
          <button class="btn" class:primary={c === first} use:autofocus={c === first} onclick={() => choose(c)}>{t(c === 'add' ? 'connect.addHere' : 'connect.newWindow')}</button>
        {/each}
      </div>
    </div>
  {:else if !started}
    <input class="query" use:focus bind:value={query} {onkeydown} placeholder={t('connect.placeholder')} spellcheck="false" autocomplete="off" />
    <ul role="listbox" aria-label={t('connect.title')}>
      {#each rows as row, i (row.key)}
        {@const label = sectionLabel[row.section]}
        {#if label && rows[i - 1]?.section !== row.section}
          <li class="section" role="presentation">{t(label)}</li>
        {/if}
        <li
          role="option"
          aria-selected={i === active}
          class:active={i === active}
          onmousemove={() => (active = i)}
          onclick={() => connect(row.target)}
          onkeydown={() => {}}
        >
          {#if row.section === 'local'}<Monitor size={14} />{:else if row.section === 'recent'}<Clock size={14} />{:else if row.section === 'typed'}<ArrowRight size={14} />{:else}<Server size={14} />{/if}
          <span class="name">{row.section === 'local' ? t('connect.local') : row.target.label}</span>
          {#if row.detail}<span class="detail mono">{row.detail}</span>{/if}
          {#if row.section === 'typed'}<span class="detail">{t('connect.go')}</span>{/if}
        </li>
      {/each}
      {#if !hosts.length && !query}
        <li class="section" role="presentation">{t('connect.sshConfig')}</li>
        <li class="empty" role="presentation">{t('connect.noHosts')}</li>
      {/if}
    </ul>
    {#if demo || canDemo}
      <div class="demo">
        <span>{t(demo ? 'demo.exitHint' : 'demo.tryHint')}</span>
        <button class="btn" onclick={() => (demo ? exitDemo() : startDemo(app.client))}>{t(demo ? 'demo.exit' : 'demo.try')}</button>
      </div>
    {/if}
  {:else}
    <div class="progress">
      <div class="target">
        {#if lastTarget?.kind === 'local'}<Monitor size={15} />{:else}<Server size={15} />{/if}
        <span>{lastTarget?.kind === 'local' ? t('connect.local') : lastTarget?.label}</span>
      </div>
      {#if app.conn.resolved}<p class="resolved mono" title={app.conn.resolved}>{app.conn.resolved}</p>{/if}
      <ol>
        {#each stepList as s (s.id)}
          <li class={s.status}>
            {#if s.status === 'done'}<Check size={14} />{:else if s.status === 'active'}<span class="spin"><Loader size={14} /></span>{:else if s.status === 'failed'}<CircleX size={14} />{:else if s.status === 'skipped'}<Minus size={14} />{:else}<Circle size={14} />{/if}
            <span>{stepLabel(s.id)}</span>
            {#if s.status === 'active' && app.conn.detail}<span class="detail">{app.conn.detail}</span>{/if}
            {#if s.status === 'active' && app.conn.phase === 'reconnecting'}<span class="detail">{t('conn.reconnecting', { n: app.conn.attempt ?? 1 })}</span>{/if}
          </li>
        {/each}
      </ol>
      {#if app.conn.prompt}
        {#key app.conn.prompt.id}<AuthPrompt prompt={app.conn.prompt} />{/key}
      {/if}
      {#if app.conn.phase === 'error' && lnReport}
        <LocalNetworkPanel report={lnReport} view={lnView} error={app.conn.error} onretry={() => connect(lastTarget ?? undefined)} onback={() => (started = false)} />
      {:else if app.conn.phase === 'error'}
        <div class="error" role="alert">
          <strong>{t('conn.error')}</strong>
          {#if app.conn.resolved}<p class="resolved mono">{app.conn.resolved}</p>{/if}
          <pre>{app.conn.error}</pre>
        </div>
        <div class="actions">
          {#if canDemo}<button class="btn" onclick={() => startDemo(app.client)}>{t('demo.try')}</button>{/if}
          <button class="btn" onclick={() => (started = false)}>{t('common.back')}</button>
          <button class="btn primary" onclick={() => connect(lastTarget ?? undefined)}>{t('common.retry')}</button>
        </div>
      {/if}
    </div>
  {/if}
</Modal>

<style>
  .query {
    width: 100%;
    height: 34px;
    margin-bottom: 8px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 46vh;
    overflow: auto;
  }
  li[role='option'] {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius);
    cursor: pointer;
    color: var(--text-2);
  }
  li.active {
    background: var(--accent-soft);
    color: var(--text);
  }
  .name {
    color: var(--text);
  }
  .detail {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-3);
  }
  .section {
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
    padding: 10px 10px 4px;
  }
  .empty {
    padding: 4px 10px;
    color: var(--text-3);
  }
  .choose {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .remember {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    font-size: 13px;
  }
  .progress {
    padding: 4px 0;
  }
  .target {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
    margin-bottom: 12px;
  }
  .resolved {
    margin: -6px 0 12px;
    font-size: 11px;
    color: var(--text-3);
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  .error .resolved {
    margin: 6px 0 0;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }
  ol li {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-3);
    transition: color var(--ease);
  }
  ol li.done,
  ol li.active {
    color: var(--text);
  }
  ol li.done :global(svg) {
    color: var(--st-input);
  }
  ol li.active :global(svg) {
    color: var(--accent);
  }
  ol li.failed {
    color: var(--st-failed);
  }
  ol li.skipped {
    text-decoration: line-through;
  }
  .spin {
    display: grid;
  }
  .error {
    margin-top: 14px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-left: 2px solid var(--st-failed);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .error strong {
    font-weight: 600;
    color: var(--st-failed);
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
  .demo {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 10px;
    padding: 10px 10px 2px;
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .demo .btn {
    flex: none;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
