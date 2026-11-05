<script lang="ts">
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FolderX from '@lucide/svelte/icons/folder-x';
  import Plus from '@lucide/svelte/icons/plus';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import Command from '@lucide/svelte/icons/command';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { settings } from '../../settings.svelte';
  import { emptyPane } from '../../emptypane';
  import { shortPath } from '../../dirs';
  import { hint } from '../../keyboard';
  import AgentGlyph from '../ui/AgentGlyph.svelte';

  const view = $derived(
    emptyPane({
      sidebarMode: app.sidebarMode,
      workspaceId: app.workspaceId,
      workspace: app.currentWorkspace,
      members: app.sidebarMode === 'workspaces' && !app.workspaceId.startsWith('group:') ? app.membersOf(app.workspaceId).length : 0,
      agents: app.agents,
      defaultAgent: settings.defaultAgent,
      connected: app.connected,
    }),
  );
  const ws = $derived(view.workspace);

  function start(id: string) {
    app.newSession(id).catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
  }
</script>

<div class="empty-pane" class:scoped={!!ws}>
  {#if ws}
    <div class="head">
      <span class="badge" class:missing={ws.missing}>
        {#if ws.missing}<FolderX size={22} strokeWidth={1.5} />{:else}<FolderOpen size={22} strokeWidth={1.5} />{/if}
      </span>
      <h2>{ws.name}</h2>
      <p class="path mono faint" title={ws.path}>{shortPath(ws.path)}</p>
    </div>
    <p class="lead muted">{ws.missing ? t('pane.missing') : view.noSessions ? t('pane.wsEmpty') : t('pane.wsIdle')}</p>
  {:else}
    <div class="head">
      <span class="badge"><SquareTerminal size={22} strokeWidth={1.5} /></span>
      <h2>{t('pane.emptyTitle')}</h2>
    </div>
    <p class="lead muted">{t('term.empty')}</p>
  {/if}

  <div class="actions">
    {#if view.agent}
      <button class="btn primary" disabled={!view.canStart} onclick={() => start(view.agent!.id)}>
        <AgentGlyph agent={view.agent.id} size={14} />{t('pane.start', { name: view.agent.name })}
      </button>
    {/if}
    <button class="btn" disabled={!view.canStart} onclick={() => start('shell')}>
      <AgentGlyph agent="shell" size={14} />{t('tabs.shell')}
    </button>
    <button class="btn" onclick={() => window.dispatchEvent(new CustomEvent('sb:newtab'))}>
      <Plus size={14} />{t('pane.more')}
    </button>
  </div>

  <div class="keys faint">
    <span><kbd>{hint('newTab')}</kbd>{t('tabs.new')}</span>
    <span><kbd>{hint('palette')}</kbd><Command size={12} />{t('pane.palette')}</span>
  </div>
</div>

<style>
  .empty-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 24px 16px;
    min-height: 0;
    overflow: auto;
    text-align: center;
  }
  .head {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    max-width: 100%;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-2);
    margin-bottom: 4px;
  }
  .badge.missing {
    color: var(--st-failed);
  }
  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .path {
    margin: 0;
    font-size: 12px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lead {
    margin: 0;
    max-width: 420px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px 16px;
    font-size: 12px;
  }
  :global(:root.phone) .keys {
    display: none;
  }
  .keys span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  kbd {
    font-family: var(--mono, monospace);
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--border);
    color: var(--text-2);
  }
</style>
