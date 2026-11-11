<script lang="ts">
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Check from '@lucide/svelte/icons/check';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { agentName, canInstall } from '../../agents';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { AgentInfo, DaemonEnv } from '../../types';
  import AgentGlyph from './AgentGlyph.svelte';

  let {
    agents,
    value,
    label,
    onchange,
    open = $bindable(false),
  }: { agents: AgentInfo[]; value: string; label: string; onchange: (id: string) => void; open?: boolean } = $props();

  const uid = `ap-${Math.random().toString(36).slice(2, 8)}`;
  let active = $state(0);
  let root: HTMLDivElement;
  let list: HTMLUListElement | undefined = $state();
  let pos = $state({ top: 0, left: 0, width: 0, up: false });
  let rescanning = $state(false);
  let env = $state<DaemonEnv | null>(null);

  const current = $derived(agents.find((a) => a.id === value));

  function place() {
    const r = root.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    const up = below < 240 && r.top > below;
    pos = { top: up ? r.top - 4 : r.bottom + 4, left: r.left, width: Math.max(r.width, 260), up };
  }

  function show() {
    place();
    open = true;
    if (!env && app.connected) app.api.env().then((e) => (env = e), () => {});
    active = Math.max(0, agents.findIndex((a) => a.id === value));
    requestAnimationFrame(() => list?.focus({ preventScroll: true }));
  }

  function choose(a: AgentInfo | undefined) {
    if (a && canInstall(a)) return install(a);
    if (!a?.installed) return;
    open = false;
    onchange(a.id);
    root.querySelector<HTMLElement>('.trigger')?.focus();
  }

  function install(a: AgentInfo) {
    open = false;
    app.installAgent(a.id).catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
  }

  async function rescan() {
    if (rescanning || !app.connected) return;
    rescanning = true;
    try {
      app.agents = await app.api.rescanAgents();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      rescanning = false;
    }
  }

  function step(from: number, dir: number): number {
    for (let i = 1; i <= agents.length; i++) {
      const j = (from + dir * i + agents.length * i) % agents.length;
      if (agents[j].installed || canInstall(agents[j])) return j;
    }
    return from;
  }

  function onListKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') active = step(active, 1);
    else if (e.key === 'ArrowUp') active = step(active, -1);
    else if (e.key === 'Home') active = step(-1, 1);
    else if (e.key === 'End') active = step(agents.length, -1);
    else if (e.key === 'Enter' || e.key === ' ') choose(agents[active]);
    else if (e.key === 'Escape' || e.key === 'Tab') {
      open = false;
      if (e.key === 'Tab') return;
      root.querySelector<HTMLElement>('.trigger')?.focus();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      show();
    }
  }

  $effect(() => {
    const el = open ? list?.querySelector<HTMLElement>(`[data-i="${active}"]`) : null;
    if (!el || !list) return;
    if (el.offsetTop < list.scrollTop) list.scrollTop = el.offsetTop;
    else if (el.offsetTop + el.offsetHeight > list.scrollTop + list.clientHeight) list.scrollTop = el.offsetTop + el.offsetHeight - list.clientHeight;
  });
</script>

<svelte:window onpointerdown={(e) => open && !root.contains(e.target as Node) && (open = false)} onresize={() => open && place()} />

<div class="picker" bind:this={root}>
  <button
    type="button"
    class="trigger"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={uid}
    aria-label="{label}: {agentName(agents, value)}"
    onclick={() => (open ? (open = false) : show())}
    onkeydown={onTriggerKey}
  >
    <AgentGlyph agent={value} size={14} />
    <span class="name">{agentName(agents, value)}</span>
    {#if current?.version}<span class="ver mono">{current.version}</span>{/if}
    <ChevronDown size={13} />
  </button>
  {#if open}
    <div class="pop" class:up={pos.up} style:top="{pos.top}px" style:left="{pos.left}px" style:width="{pos.width}px">
      <ul
        id={uid}
        class="list"
        style:max-height="{Math.min(368, (pos.up ? pos.top : innerHeight - pos.top) - 12) - (app.connected ? 32 : 0)}px"
        role="listbox"
        tabindex="-1"
        aria-label={label}
        aria-activedescendant="{uid}-{active}"
        bind:this={list}
        onkeydown={onListKey}
      >
        {#each agents as a, i (a.id)}
          <li
            id="{uid}-{i}"
            data-i={i}
            role="option"
            aria-selected={a.id === value}
            aria-disabled={!a.installed && !canInstall(a)}
            class:active={i === active}
            class:off={!a.installed}
            class:can={canInstall(a)}
            title={a.installed ? (a.version ?? a.name) : canInstall(a) ? t('agents.installIn', { name: a.name, cmd: a.install ?? '' }) : `${t('agents.notInstalled')} · ${a.install ?? ''}`}
            onpointermove={() => (a.installed || canInstall(a)) && (active = i)}
            onclick={() => choose(a)}
            onkeydown={() => {}}
          >
            <span class="glyph"><AgentGlyph agent={a.id} size={14} /></span>
            <span class="text">
              <span class="name">{a.name}</span>
              {#if !a.installed}
                <span class="sub mono">{a.install ?? t('agents.notInstalled')}</span>
              {:else if a.version}
                <span class="sub">{a.version}</span>
              {/if}
            </span>
            {#if a.id === value}<Check size={13} />{:else if canInstall(a)}<span class="act">{t('agents.install')}</span>{/if}
          </li>
        {/each}
      </ul>
      {#if app.connected}
        <div class="foot">
          <button type="button" class="rescan" disabled={rescanning} title={env ? t('agents.pathSource', { source: env.source }) : undefined} onclick={rescan}>
            <span class:spin={rescanning}><RefreshCw size={12} /></span>
            {rescanning ? t('agents.rescanning') : t('agents.rescan')}
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 30px;
    padding: 0 8px 0 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    text-align: left;
    transition: border-color var(--ease);
  }
  .trigger:hover {
    border-color: var(--border-strong);
  }
  .trigger[aria-expanded='true'],
  .trigger:focus-visible {
    border-color: var(--accent);
    outline: none;
  }
  .trigger .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trigger :global(svg:last-child) {
    color: var(--text-3);
  }
  .ver {
    font-size: 10.5px;
    color: var(--text-3);
  }
  .pop {
    position: fixed;
    z-index: 70;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .pop.up {
    transform: translateY(-100%);
  }
  .list {
    overflow: auto;
    margin: 0;
    padding: 4px;
    list-style: none;
    outline: none;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    height: 32px;
    padding: 0 6px;
    align-items: center;
    border-top: 1px solid var(--border);
  }
  .rescan {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .rescan:hover:not(:disabled) {
    background: var(--surface-hover);
    color: var(--text);
  }
  .rescan:disabled {
    cursor: default;
    color: var(--text-3);
  }
  .rescan span {
    display: inline-grid;
  }
  @media (prefers-reduced-motion: no-preference) {
    .spin {
      animation: spin 0.9s linear infinite;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 8px;
    border-radius: 4px;
    cursor: pointer;
    color: var(--text);
  }
  li.active {
    background: var(--accent-soft);
  }
  li.off {
    cursor: default;
    color: var(--text-3);
  }
  li.off.can {
    cursor: pointer;
  }
  .act {
    font-size: 10.5px;
    color: var(--accent);
    flex: none;
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--text-2);
    flex: none;
  }
  .off .glyph {
    background: transparent;
    color: var(--text-3);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .text .name {
    line-height: 17px;
  }
  .sub {
    font-size: 11px;
    line-height: 15px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub.mono {
    font-size: 10.5px;
  }
  li > :global(svg) {
    color: var(--accent);
    flex: none;
  }
</style>
