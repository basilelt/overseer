<script lang="ts">
  import { onMount } from 'svelte';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { canInstall, splitInstalled } from '../../agents';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import type { AgentInfo } from '../../types';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { preload } from '../../idle';

  let { onclose }: { onclose: (refocus: boolean) => void } = $props();

  const profileSelect = preload(() => import('../ui/ProfileSelect.svelte'));
  let menu: HTMLDivElement | undefined = $state();
  const split = $derived(splitInstalled(app.agents));
  let more = $state(false);

  onMount(() => {
    menu?.querySelector<HTMLElement>('button:not([disabled])')?.focus();
  });

  async function start(a: AgentInfo) {
    onclose(false);
    try {
      await (a.installed ? app.newSession(a.id) : app.installAgent(a.id));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  function onKey(e: KeyboardEvent) {
    const items = [...(menu?.querySelectorAll<HTMLElement>('button:not([disabled])') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'ArrowDown') items[(i + 1) % items.length]?.focus();
    else if (e.key === 'ArrowUp') items[(i - 1 + items.length) % items.length]?.focus();
    else if (e.key === 'Escape') onclose(true);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div class="menu" role="menu" tabindex="-1" aria-label={t('title.new')} bind:this={menu} onkeydown={onKey}>
  {#each split.installed as a (a.id)}
    <button role="menuitem" title={a.version ?? a.name} onclick={() => start(a)}>
      <span class="glyph"><AgentGlyph agent={a.id} size={14} /></span>
      <span class="text"><span>{a.name}</span></span>
      {#if a.version}<span class="ver mono">{a.version}</span>{/if}
    </button>
  {/each}
  {#if split.missing.length}
    {#if split.installed.length}<hr />{/if}
    <button role="menuitem" class="more" aria-expanded={more} onclick={() => (more = !more)}>
      <span class="glyph chev" class:open={more}><ChevronRight size={14} /></span>
      <span class="text"><span>{t('agents.notInstalledCount', { n: split.missing.length })}</span></span>
    </button>
    {#if more}
      {#each split.missing as a (a.id)}
        <button
          role="menuitem"
          disabled={!canInstall(a)}
          title={canInstall(a) ? t('agents.installIn', { name: a.name, cmd: a.install ?? '' }) : `${t('agents.notInstalled')} · ${a.install ?? ''}`}
          onclick={() => start(a)}
        >
          <span class="glyph"><AgentGlyph agent={a.id} size={14} /></span>
          <span class="text">
            <span>{a.name}</span>
            <span class="sub mono">{a.install ?? t('agents.notInstalled')}</span>
          </span>
          {#if canInstall(a)}<span class="act">{t('agents.install')}</span>{/if}
        </button>
      {/each}
    {/if}
  {/if}
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

<style>
  .menu {
    position: absolute;
    top: 32px;
    left: 0;
    width: 280px;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    z-index: var(--z-dropdown);
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
  .more {
    color: var(--text-2);
  }
  .chev {
    background: transparent;
    transition: transform 120ms;
  }
  .chev.open {
    transform: rotate(90deg);
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
  .act {
    font-size: 10.5px;
    color: var(--accent);
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
