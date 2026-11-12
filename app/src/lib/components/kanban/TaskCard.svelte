<script lang="ts" module>
  import { ensureSprite } from '../../icons/sprite';
  import { formatCost } from '../../usage/format';

  ensureSprite();
</script>

<script lang="ts">
  import QuorumChips from './QuorumChips.svelte';
  import GitBranch from '@lucide/svelte/icons/git-branch';
  import FolderGit from '@lucide/svelte/icons/folder-git';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { t, type Key } from '../../i18n.svelte';
  import { cardMeta, isSession, modelLabel, quadrant, relTime, type CardStatus } from '../../kanban/kanban';
  import type { Task } from '../../types';
  import { app, projectName } from '../../state.svelte';
  import { armedConfirm } from '../../armed-confirm.svelte';
  import { agentName, findAgent } from '../../agents';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { isDone, isPending, isRunning, isTesting, isUnstarted } from '../../kanban/task-status';

  let {
    task,
    color,
    status,
    dragging = false,
    lifted = false,
    ghost = false,
    filtered = false,
    showStatus = false,
    showQuadrant = true,
    chained = false,
    onqueue,
    ondelete,
    onstart,
    onopen,
    ondetails,
    onmore,
    onrename,
    onchain,
    onlinks,
    onpointerdown,
    onkeydown,
  }: {
    task: Task;
    color: string;
    status: CardStatus;
    filtered?: boolean;
    showStatus?: boolean;
    showQuadrant?: boolean;
    dragging?: boolean;
    lifted?: boolean;
    ghost?: boolean;
    chained?: boolean;
    onqueue?: () => void;
    ondelete?: () => void;
    onstart?: (inPlace: boolean) => void;
    onopen?: () => void;
    ondetails?: () => void;
    onmore?: () => void;
    onrename?: (title: string) => void;
    onchain?: () => void;
    onlinks?: (on: boolean) => void;
    onpointerdown?: (e: PointerEvent) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();

  const confirm = armedConfirm();
  let editing = $state(false);
  let draft = $state('');
  let openTimer: ReturnType<typeof setTimeout> | undefined;

  const age = $derived(relTime(task.updatedAt));
  const session = $derived(isSession(task));
  const openable = $derived(isTesting(task.status) || (session ? !!task.lineId : isRunning(task.status)));
  const meta = $derived(cardMeta(task, { filtered, defaultBranch: app.allProjects.find((p) => p.path === task.project)?.defaultBranch }));
  const model = $derived(modelLabel(task.model) || agentName(app.agents, task.agent));
  const quad = $derived(showQuadrant ? quadrant(task) : 'none');
  const parent = $derived(task.origin === 'agent' && task.parentId ? app.data.tasks.find((x) => x.id === task.parentId) : undefined);
  const renamable = $derived(!!onrename && !ghost);
  const deps = $derived((task.dependsOn ?? []).map((id) => app.data.tasks.find((x) => x.id === id)).filter((x) => !!x));

  function onCardClick(e: MouseEvent) {
    const go = openable ? onopen : session ? undefined : ondetails;
    if (!go || editing) return;
    clearTimeout(openTimer);
    if (renamable && (e.target as HTMLElement).closest('.title')) openTimer = setTimeout(go, 250);
    else go();
  }

  function startRename(e: MouseEvent) {
    if (!renamable) return;
    e.stopPropagation();
    clearTimeout(openTimer);
    draft = task.title;
    editing = true;
  }

  function finishRename(save: boolean) {
    if (!editing) return;
    editing = false;
    const title = draft.trim();
    if (save && title && title !== task.title) onrename?.(title);
  }

  function focusSelect(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  function askDelete(e: MouseEvent) {
    e.stopPropagation();
    if (confirm.press()) ondelete?.();
  }

  function act(fn?: () => void) {
    return (e: MouseEvent) => {
      e.stopPropagation();
      fn?.();
    };
  }
</script>

<div class="li" role="listitem">
  <div
    class="card"
    class:dragging
    class:lifted
    class:ghost
    class:chained
    class:openable={openable || (!session && !!ondetails)}
    role="button"
    tabindex={ghost ? -1 : 0}
    data-task={task.id}
    aria-roledescription="draggable task"
    aria-grabbed={lifted}
    aria-label="#{task.number} {task.title}"
    onclick={onCardClick}
    ondblclick={(e) => (e.target as HTMLElement).closest('.title') && startRename(e)}
    {onpointerdown}
    onpointerenter={(e) => e.pointerType === 'mouse' && onlinks?.(true)}
    onpointerleave={(e) => e.pointerType === 'mouse' && onlinks?.(false)}
    onkeydown={(e) => {
      if (e.key === 'F2' && renamable && !editing && e.target === e.currentTarget) {
        e.preventDefault();
        draft = task.title;
        editing = true;
      } else onkeydown?.(e);
    }}
  >
    <div class="top">
      {#if session}<span class="session" title={t('kanban.sessionTask')} aria-label={t('kanban.sessionTask')}><svg class="i" width="11" height="11" aria-hidden="true"><use href="#sbi-radio" /></svg></span>{/if}
      {#if meta.project}<span class="project"><span class="dot" style:background={color}></span>{projectName(app.allProjects, task.project)}</span>{/if}
      <span class="num mono">#{task.number}</span>
      {#if task.ticketKey}<span class="ticket mono">{task.ticketKey}</span>{/if}
      {#if status.kind === 'blocked'}<span class="blocked"><svg class="i" width="10" height="10" aria-hidden="true"><use href="#sbi-lock" /></svg>{t('kanban.blocked')}</span>{/if}
      {#if deps.length}
        <button
          type="button"
          class="links"
          title={t('kanban.deps', { list: deps.map((d) => `#${d.number}`).join(', ') })}
          aria-label={t('kanban.deps', { list: deps.map((d) => `#${d.number}`).join(', ') })}
          aria-pressed={chained}
          onpointerdown={(e) => e.stopPropagation()}
          onclick={(e) => {
            e.stopPropagation();
            onlinks?.(!chained);
          }}><svg class="i" width="11" height="11" aria-hidden="true"><use href="#sbi-link" /></svg>{deps.length}</button>
      {/if}
      {#if quad !== 'none'}<span class="quad {quad}">{t(`kanban.q.${quad}` as Key)}</span>{/if}
      {#if task.origin === 'agent'}
        <span class="by" title={parent ? t('kanban.byTaskTitle', { n: parent.number }) : t('task.createdByAgent')}><svg class="i" width="11" height="11" aria-hidden="true"><use href="#sbi-bot" /></svg>{#if parent}{t('kanban.byTask', { n: parent.number })}{/if}</span>
      {/if}
      <QuorumChips {task} />
      {#if showStatus}<span class="stat {task.status}">{t(`kanban.col.${task.status}` as Key)}</span>{/if}
    </div>
    {#if editing}
      <input
        class="title-input"
        aria-label={t('kanban.renameLabel')}
        bind:value={draft}
        use:focusSelect
        onpointerdown={(e) => e.stopPropagation()}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') finishRename(true);
          else if (e.key === 'Escape') finishRename(false);
        }}
        onblur={() => finishRename(true)}
      />
    {:else}
      <div class="title" title={renamable ? t('kanban.renameHint') : undefined}>{task.title}</div>
    {/if}
    {#if !session && task.description}<div class="desc">{task.description}</div>{/if}

    {#if status.kind === 'working' || status.kind === 'progress'}
      <div class="status working" title={status.note}><span class="pulse"></span><span class="note">{status.note || t('kanban.working')}</span></div>
    {:else if status.kind === 'input'}
      <div class="status input"><span class="dot"></span>{t('kanban.needsInput')}</div>
    {:else if status.kind === 'paused'}
      <div class="status paused"><svg class="i" width="11" height="11" aria-hidden="true"><use href="#sbi-pause" /></svg>{t('kanban.paused')}</div>
    {:else if status.kind === 'starting'}
      <div class="status working"><LoaderCircle size={12} class="spin" />{t('kanban.starting')}</div>
    {:else if status.kind === 'blocked'}
      <div class="status waiting"><span class="note">{t('kanban.waitingOn', { list: (status.waiting ?? []).map((n) => `#${n}`).join(', ') })}</span></div>
    {:else if status.kind === 'next'}
      <div class="status next"><span class="dot"></span>{t('kanban.nextUp')}</div>
    {/if}

    <div class="meta">
      <span class="agent" title={[agentName(app.agents, task.agent), task.model].filter(Boolean).join(' · ')}>
        <AgentGlyph agent={task.agent} size={12} />
        {model}
      </span>
      {#if meta.branch}<span class="ref mono" title={meta.branch}><svg class="i" width="11" height="11" aria-hidden="true"><use href="#sbi-git-branch" /></svg><span>{meta.branch}</span></span>{/if}
      {#if meta.worktree}<span class="ref mono" title={task.worktreePath}><svg class="i" width="11" height="11" aria-hidden="true"><use href="#sbi-folder-git" /></svg><span>{meta.worktree}</span></span>{/if}
      {#if status.kind === 'progress'}
        <span class="progress" title="{status.pct}%"><span class="bar"><span class="fill" style:width="{status.pct}%"></span></span><span class="pct mono">{task.progress?.done}/{task.progress?.total}</span></span>
      {/if}
      {#if task.cost}<span class="cost mono" title={t('cost.task')}>{formatCost(task.cost.cost, task.cost.partial)}</span>{/if}
      {#if !session && task.effort && (findAgent(app.agents, task.agent)?.efforts.length ?? 1)}<span class="effort">{t(`effort.${task.effort}` as Key)}</span>{/if}
      <span class="tail">
      <span class="age faint">{t(age.key, { n: age.n })}</span>
      {#if !ghost && (!session || onmore)}
        <span class="actions">
          {#if !session && isPending(task.status)}
            <button class="act" title={t('kanban.queueAuto')} aria-label={t('kanban.queueAuto')} onclick={act(onqueue)}><svg class="i" width="13" height="13" aria-hidden="true"><use href="#sbi-zap" /></svg></button>
          {/if}
          {#if !session && isUnstarted(task.status)}
            <button
              class="act"
              title={t('kanban.start')}
              aria-label={t('kanban.start')}
              aria-haspopup="menu"
              onclick={(e) => {
                e.stopPropagation();
                app.openMenu(e, t('kanban.start'), [
                  { label: t('kanban.startWorktree'), icon: GitBranch, run: () => onstart?.(false) },
                  { label: t('kanban.startCheckout'), icon: FolderGit, run: () => onstart?.(true) },
                ]);
              }}><svg class="i" width="13" height="13" aria-hidden="true"><use href="#sbi-play" /></svg></button>
          {/if}
          {#if !session && onchain && !isDone(task.status)}
            <button class="act" title={t('kanban.chain')} aria-label={t('kanban.chain')} onclick={act(onchain)}><svg class="i" width="13" height="13" aria-hidden="true"><use href="#sbi-corner-down-right" /></svg></button>
          {/if}
          {#if !session && ondetails}
            <button class="act" title={t('kanban.details')} aria-label={t('kanban.details')} onclick={act(ondetails)}><svg class="i" width="13" height="13" aria-hidden="true"><use href="#sbi-message-square" /></svg></button>
          {/if}
          {#if !session}
            <button
              class="act del"
              class:confirm={confirm.armed}
              title={confirm.armed ? t('kanban.deleteConfirm', { n: task.number }) : t('common.delete')}
              aria-label={confirm.armed ? t('kanban.deleteConfirm', { n: task.number }) : t('common.delete')}
              onclick={askDelete}
            >
              <svg class="i" width="13" height="13" aria-hidden="true"><use href="#sbi-trash" /></svg>{#if confirm.armed}<span>{t('common.confirm')}</span>{/if}
            </button>
          {/if}
          {#if onmore}
            <button class="act" title={t('mobile.tasks.more', { n: task.number })} aria-label={t('mobile.tasks.more', { n: task.number })} aria-haspopup="dialog" onclick={act(onmore)}><svg class="i" width="15" height="15" aria-hidden="true"><use href="#sbi-ellipsis" /></svg></button>
          {/if}
        </span>
      {/if}
      </span>
    </div>
  </div>
</div>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 11px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: grab;
    touch-action: none;
    transition:
      border-color var(--ease),
      opacity var(--ease),
      box-shadow var(--ease);
    content-visibility: auto;
    contain-intrinsic-size: auto 118px;
  }
  .card:hover {
    border-color: var(--border-strong);
  }
  .card.openable {
    cursor: pointer;
  }
  .card:focus-visible {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--focus);
  }
  .card.chained {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .card.dragging {
    opacity: 0.35;
  }
  .card.lifted {
    border-color: var(--accent);
    box-shadow: var(--shadow);
  }
  .card.ghost {
    box-shadow: var(--shadow);
    cursor: grabbing;
    content-visibility: visible;
    transform: rotate(1.2deg);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-2);
    min-width: 0;
    flex-wrap: wrap;
    row-gap: 3px;
  }
  .project {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .session {
    display: inline-flex;
    flex: none;
    color: var(--text-3);
  }
  .num,
  .ticket {
    font-size: 10.5px;
    color: var(--text-3);
  }
  .quad,
  .by,
  .blocked,
  .links,
  .stat {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: none;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 10.5px;
    line-height: 16px;
    white-space: nowrap;
  }
  .quad {
    --c: var(--text-3);
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, transparent);
  }
  .quad.do {
    --c: var(--st-failed);
  }
  .quad.plan {
    --c: var(--st-testing);
  }
  .quad.delegate {
    --c: var(--st-input);
  }
  .by {
    color: var(--text-2);
    border: 1px solid var(--border);
  }
  .blocked {
    color: var(--st-failed);
    background: color-mix(in srgb, var(--st-failed) 12%, transparent);
  }
  .links {
    color: var(--text-2);
    border: 1px solid var(--border);
    font-family: inherit;
  }
  .links:hover,
  .links[aria-pressed='true'] {
    color: var(--accent);
    border-color: var(--accent);
  }
  .stat {
    margin-left: auto;
    color: var(--text-2);
    background: var(--surface-2);
  }
  .age {
    margin-left: auto;
    flex: none;
  }
  .title {
    font-weight: 500;
    line-height: 1.35;
    color: var(--text);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .title-input {
    font: inherit;
    font-weight: 500;
    line-height: 1.35;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--accent);
    border-radius: 4px;
    padding: 1px 5px;
    margin: -2px -6px;
    outline: none;
    box-shadow: 0 0 0 2px var(--focus);
    min-width: 0;
  }
  .desc {
    color: var(--text-2);
    font-size: 12px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-2);
    min-width: 0;
  }
  .status .pulse {
    flex: none;
  }
  .status.working {
    color: var(--st-working);
  }
  .status.input {
    color: var(--st-input);
  }
  .status.input .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--st-input);
  }
  .status.waiting {
    color: var(--text-3);
  }
  .status.paused {
    color: var(--text-3);
  }
  .pulse {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--st-working);
    animation: pulse 1.6s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse {
      animation: none;
    }
  }
  .status.next .dot {
    width: 6px;
    height: 6px;
    background: var(--st-working);
  }
  .progress {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }
  .bar {
    flex: 0 0 36px;
    width: 36px;
    height: 4px;
    border-radius: 2px;
    background: var(--surface-active);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--st-working);
    transition: width 300ms ease;
  }
  .pct {
    flex: none;
    font-size: 10.5px;
    color: var(--text-3);
  }
  .note {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .meta {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-2);
    min-height: 22px;
    min-width: 0;
    flex-wrap: wrap;
    row-gap: 4px;
  }
  .agent {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    white-space: nowrap;
  }
  .ref {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    min-width: 6em;
    max-width: 100%;
    flex: 1 1 6em;
    font-size: 10.5px;
    color: var(--text-3);
  }
  .ref > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tail {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
    margin-left: auto;
  }
  .meta .age {
    color: var(--text-3);
    padding-left: 4px;
  }
  .cost {
    flex: none;
    font-size: 11px;
    color: var(--text-3);
  }
  .effort {
    flex: none;
    white-space: nowrap;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    line-height: 16px;
  }
  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 2px;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--ease);
  }
  .card:hover .actions,
  .card:focus-within .actions,
  .card:focus .actions {
    opacity: 1;
    pointer-events: auto;
  }
  .act {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    min-width: 22px;
    padding: 0 4px;
    justify-content: center;
    border-radius: 4px;
    color: var(--text-2);
    transition:
      background var(--ease),
      color var(--ease);
  }
  .act:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .act.del:hover,
  .act.confirm {
    color: var(--st-failed);
  }
  .act.confirm {
    background: var(--diff-del-bg);
    font-size: 11px;
  }
  :global(.phone) .card {
    padding: 12px 12px 8px;
    touch-action: pan-y;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    content-visibility: visible;
  }
  :global(.phone) .card:hover {
    border-color: var(--border);
  }
  :global(.phone) .actions {
    opacity: 1;
    pointer-events: auto;
  }
  :global(.phone) .act {
    height: 44px;
    min-width: 44px;
  }
  :global(.phone) .links {
    min-height: 28px;
    padding: 0 8px;
  }
  :global(.phone) .title {
    font-size: 14px;
  }
</style>
