<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import PanelRight from '@lucide/svelte/icons/panel-right';
  import Globe from '@lucide/svelte/icons/globe';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import FolderTree from '@lucide/svelte/icons/folder-tree';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { tabLabel } from '../../tabs/tab-label';
  import { closeTabKey, command, selectTabKey } from '../../commands/commands';
  import { tabInfo, type TabInfo } from '../../tabs/tabs';
  import { activeTabs, workspace } from '../../workspace/workspace.svelte';
  import { longPress } from '../../mobile/longpress';
  import Pin from '@lucide/svelte/icons/pin';
  import PinOff from '@lucide/svelte/icons/pin-off';
  import X from '@lucide/svelte/icons/x';
  import type { MenuItem } from '../../commands/menu';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import { chatCapable } from '../../chat/viewmode';
  import { modeOf, setMode } from '../../chat/viewmode.svelte';
  import { pages } from '../../session/session-pages.svelte';
  import { GROUP_COLOR } from '../../workspace/ws-color';

  const tabs = $derived(activeTabs(workspace.layout).map((k) => tabInfo(k, app.items)).filter((x): x is TabInfo => !!x));
  const canPanel = $derived(!!app.selected?.lineId);
  const web = $derived(app.selected ? pages.byKey[app.selected.key] : undefined);
  const chatItem = $derived(app.selected && chatCapable(app.selected) ? app.selected : null);
  const inChat = $derived(chatItem ? modeOf(chatItem) === 'chat' : false);
  let strip: HTMLDivElement | undefined = $state();

  const label = (tab: TabInfo) => tabLabel(tab, { browserHost: false });

  function chipMenu(e: PointerEvent, tab: TabInfo) {
    const line = tab.item?.lineId ? app.data.lines.find((l) => l.line.id === tab.item?.lineId) : undefined;
    const items: MenuItem[] = [];
    if (line?.local) {
      const pinned = !!line.local.pinned;
      items.push({
        label: t(pinned ? 'action.unpin' : 'action.pin'),
        icon: pinned ? PinOff : Pin,
        run: () => void app.api.pin(line.line.id, !pinned).catch((err) => app.notify(String((err as Error)?.message ?? err), 'error')),
      });
    }
    items.push({ label: t('common.close'), icon: X, run: () => closeTabKey(tab.key, true, true) });
    navigator.vibrate?.(10);
    app.openMenu(e, label(tab), items);
  }

  $effect(() => {
    const key = app.selectedKey;
    if (!strip || !key) return;
    strip.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)?.scrollIntoView({ block: 'nearest', inline: 'center' });
  });
</script>

<div class="chips" data-tour="tabs">
  <div class="strip" role="tablist" aria-label={t('mobile.shell.tabs')} bind:this={strip}>
    {#each tabs as tab (tab.key)}
      <button role="tab" class="chip" class:on={tab.key === app.selectedKey} aria-selected={tab.key === app.selectedKey} data-key={tab.key} use:longPress={(e) => chipMenu(e, tab)} onclick={() => selectTabKey(tab.key)}>
        {#if tab.item}
          <span class="dot" style:background={GROUP_COLOR[tab.item.group]}></span>
          <AgentGlyph agent={tab.item.kind} size={13} />
        {:else if tab.kind === 'browser'}<Globe size={13} />
        {:else if tab.kind === 'review'}<GitCompare size={13} />
        {:else if tab.kind === 'graph'}<GitBranch size={13} />
        {:else if tab.kind === 'worktrees'}<FolderTree size={13} />
        {/if}
        <span class="label">{label(tab)}</span>
      </button>
    {/each}
  </div>
  {#if web?.pages.length && app.selected}
    {@const key = app.selected.key}
    <button class="icon-btn side" class:active={web.open} aria-label={t('web.toggle')} title={t('web.toggle')} aria-pressed={web.open} onclick={() => pages.setOpen(key, !web.open)}><Globe size={18} /></button>
  {/if}
  {#if chatItem}
    <button class="icon-btn side" data-tour="view-toggle" aria-label={t(inChat ? 'chat.terminal' : 'chat.chat')} title={t(inChat ? 'chat.terminal' : 'chat.chat')} onclick={() => setMode(chatItem, inChat ? 'terminal' : 'chat')}>
      {#if inChat}<SquareTerminal size={18} />{:else}<MessageSquare size={18} />{/if}
    </button>
  {/if}
  <button class="icon-btn side" data-tour="new-session" aria-label={t('mobile.shell.newSession')} disabled={!app.connected} onclick={() => command('palette').run()}><Plus size={18} /></button>
  <button class="icon-btn side" data-tour="panel" class:active={ui.panel} aria-label={t('mobile.shell.activity')} aria-pressed={ui.panel} disabled={!canPanel} onclick={() => (ui.panel = !ui.panel)}
    ><PanelRight size={18} /></button
  >
</div>

<style>
  .chips {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 4px 6px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .strip {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    overflow-x: auto;
    scrollbar-width: none;
    scroll-snap-type: x proximity;
  }
  .strip::-webkit-scrollbar {
    display: none;
  }
  .chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 220px;
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 18px;
    background: var(--surface);
    color: var(--text-2);
    font-size: 13px;
    scroll-snap-align: center;
    -webkit-touch-callout: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
  }
  .side {
    flex: none;
    width: 44px;
    height: 44px;
  }
</style>
