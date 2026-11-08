<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import { ordered } from '../../agents';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { AgentInfo } from '../../types';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { preload } from '../../idle';

  const profileSelect = preload(() => import('../ui/ProfileSelect.svelte'));

  let open = $state(false);
  let root: HTMLDivElement;
  let menu: HTMLDivElement | undefined = $state();

  const agents = $derived(ordered(app.agents));

  function toggle() {
    open = !open;
    if (open) requestAnimationFrame(() => menu?.querySelector<HTMLElement>('button:not([disabled])')?.focus());
  }

  async function start(a: AgentInfo) {
    open = false;
    try {
      await app.newSession(a.id);
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  function onKey(e: KeyboardEvent) {
    const items = [...(menu?.querySelectorAll<HTMLElement>('button:not([disabled])') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'ArrowDown') items[(i + 1) % items.length]?.focus();
    else if (e.key === 'ArrowUp') items[(i - 1 + items.length) % items.length]?.focus();
    else if (e.key === 'Escape') {
      open = false;
      root.querySelector<HTMLElement>('.icon-btn')?.focus();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window onpointerdown={(e) => open && !root.contains(e.target as Node) && (open = false)} />

<div class="wrap" bind:this={root}>
  <button
    class="icon-btn"
    class:active={open}
    title={t('title.new')}
    aria-label={t('title.new')}
    aria-haspopup="menu"
    aria-expanded={open}
    disabled={!app.connected}
    onclick={toggle}><Plus size={15} /></button
  >
  {#if open}
    <div class="menu" role="menu" tabindex="-1" aria-label={t('title.new')} bind:this={menu} onkeydown={onKey}>
      {#each agents as a, i (a.id)}
        <button
          role="menuitem"
          disabled={!a.installed}
          title={a.installed ? (a.version ?? a.name) : `${t('agents.notInstalled')} · ${a.install ?? ''}`}
          onclick={() => start(a)}
        >
          <span class="glyph"><AgentGlyph agent={a.id} size={14} /></span>
          <span class="text">
            <span>{a.name}</span>
            {#if !a.installed}<span class="sub mono">{a.install ?? t('agents.notInstalled')}</span>{/if}
          </span>
          {#if a.installed && a.version}<span class="ver mono">{a.version}</span>{/if}
        </button>
        {#if a.installed && agents[i + 1] && !agents[i + 1].installed}<hr />{/if}
      {/each}
      {#if app.profiles}
        <hr />
        <div class="profile">
          <span class="faint">{t('profile.next')}</span>
          {#await profileSelect() then { default: ProfileSelect }}
            <ProfileSelect value={app.nextProfile ?? ''} cwd={app.currentWorkspace?.path ?? app.currentWorkspace?.cwd ?? undefined} onchange={(id) => (app.nextProfile = id || null)} />
          {/await}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: 32px;
    left: 0;
    width: 280px;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    z-index: 40;
    outline: none;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 5px 8px;
    border-radius: 4px;
    text-align: left;
    color: var(--text);
  }
  .menu button:hover:not(:disabled),
  .menu button:focus-visible {
    background: var(--accent-soft);
    outline: none;
  }
  .menu button:disabled {
    color: var(--text-3);
    cursor: default;
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
  button:disabled .glyph {
    background: transparent;
    color: var(--text-3);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .sub {
    font-size: 10.5px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ver {
    font-size: 10.5px;
    color: var(--text-3);
  }
  .profile {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px 2px;
    font-size: 11.5px;
  }
  .profile :global(select) {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
  }
  hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
</style>
