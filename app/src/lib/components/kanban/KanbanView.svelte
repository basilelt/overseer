<script lang="ts">
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import Search from '@lucide/svelte/icons/search';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Plus from '@lucide/svelte/icons/plus';
  import Radio from '@lucide/svelte/icons/radio';
  import Sparkles from '@lucide/svelte/icons/sparkles';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import MessageSquare from '@lucide/svelte/icons/message-square';
  import { isOff } from '../../session/grouping';
  import { projectName, app } from '../../state.svelte';
  import { pullAutoConcurrency } from '../../settings/auto-concurrency';
  import { saveSettings, settings } from '../../settings/settings.svelte';
  import { holdAll } from '../../workspace/wsactions';
  import { t, type Key } from '../../i18n.svelte';
  import { autoSummary, blockers, byId, canDrop, cardStatus, chainOf, columns, dropTarget, edgeSpeed, insertIndex, isSession, matrix, moveTask, nextUp, organizing, QUADRANTS, quadrantFlags, rankAt, revertTask, shiftStatus, moveAction, shownCount, grown, newTaskId, CARD_EST, frameLoop, insertBefore, type Quadrant } from '../../kanban/kanban';
  import { reveal } from '../../reveal';
  import { inFlight, single } from '../../daemon/inflight';
  import { agentName } from '../../agents';
  import type { Task, TaskStatus } from '../../types';
  import KanbanColumn from './KanbanColumn.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import TaskCard from './TaskCard.svelte';
  import Modal from '../ui/Modal.svelte';
  import ArrowUpToLine from '@lucide/svelte/icons/arrow-up-to-line';
  import ArrowDownToLine from '@lucide/svelte/icons/arrow-down-to-line';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import { isPhone } from '../../mobile/ui.svelte';
  import { LONG_PRESS_MS, pressTimer, swipeDirection } from '../../mobile/gestures';
  import { columnAfterSwipe } from '../../mobile/layout';
  import { edgeMoves, edgeRank, initialColumn, moveChoices, type Edge } from '../../mobile/kanbanPhone';
  import { tick, untrack } from 'svelte';
  import { isDone, isQueued, isRunning, isTesting } from '../../kanban/task-status';
  import { fallbackProjectColor } from './project-colors';

  let search = $state('');
  let project = $state(app.sidebarMode === 'workspaces' && app.currentWorkspace ? (app.currentWorkspace.path ?? '') : '');
  let collapsed = $state<Record<string, boolean>>({ merged: true, completed: true });
  let refreshing = $state(false);
  let live = $state('');
  let colEls = $state<Record<string, HTMLElement | undefined>>({});
  let listEls = $state<Record<string, HTMLElement | undefined>>({});
  let board = $state<HTMLElement>();

  let drag = $state.raw<{ id: string; dx: number; dy: number; w: number; target: TaskStatus | null; index: number } | null>(null);
  let pos = $state.raw({ x: 0, y: 0 });
  let pointer = { x: 0, y: 0 };
  let pending: { id: string; x: number; y: number; el: HTMLElement } | null = null;
  let suppressClick = false;
  let picked = $state<{ id: string; target: TaskStatus; index: number } | null>(null);

  const hideSession = $derived(!!settings.hideSessionTasks);
  const cols = $derived(columns(app.data.tasks, { search, project: project || undefined, hideSession }));
  const view = $derived(settings.kanbanView ?? 'board');
  const mx = $derived(matrix(app.data.tasks, { search, project: project || undefined, hideSession }));
  const orgRunning = $derived(organizing(app.data.tasks, project || undefined));
  let organizeBusy = $state(false);
  let mxDrag = $state<string | null>(null);
  let mxOver = $state<Quadrant | null>(null);
  const viewOptions = $derived([
    { value: 'board' as const, label: t('kanban.view.board') },
    { value: 'matrix' as const, label: t('kanban.view.matrix') },
  ]);
  const summary = $derived(autoSummary(app.data.tasks));
  $effect(() => {
    if (!app.connected || app.conn.phase !== 'connected') return;
    untrack(() => pullAutoConcurrency(app.api).catch(() => {}));
  });
  const next = $derived(nextUp(app.data.tasks, settings.autoConcurrency));
  const ids = $derived(byId(app.data.tasks));
  const linked = $derived(new Set(app.data.tasks.flatMap((x) => x.dependsOn ?? [])));
  let chain = $state<Set<string> | null>(null);
  let gate = $state<{ task: Task; inPlace: boolean; rank?: number; drop: boolean } | null>(null);
  const colors = $derived(new Map(app.allProjects.map((p) => [p.path, p.color])));
  const projectPaths = $derived([...new Set([...app.allProjects.map((p) => p.path), ...app.data.tasks.map((t) => t.project), ...(project ? [project] : [])])].sort());
  const dragTask = $derived(drag ? app.data.tasks.find((x) => x.id === drag!.id) : undefined);
  const dropOn = $derived(drag?.target ?? picked?.target ?? null);
  const movingId = $derived(drag?.id ?? picked?.id ?? null);
  const origin = $derived(app.data.tasks.find((x) => x.id === movingId)?.status);
  const dropAt = $derived(drag?.target ? { status: drag.target, index: drag.index } : picked ? { status: picked.target, index: picked.index } : null);
  const before = $derived(dropAt ? insertBefore(cols.find((c) => c.status === dropAt.status)?.tasks ?? [], movingId, dropAt.index) : null);

  const phone = $derived(isPhone());
  let colIndex = $state(untrack(() => initialColumn(cols.map((c) => c.status), cols.map((c) => c.tasks.length))));
  const phoneCol = $derived(cols[Math.min(colIndex, cols.length - 1)]);
  let sheetId = $state<string | null>(null);
  const sheetTask = $derived(sheetId ? app.data.tasks.find((x) => x.id === sheetId) : undefined);
  const sheetColumn = $derived(sheetTask ? (cols.find((c) => c.status === sheetTask.status)?.tasks ?? []) : []);
  const press = pressTimer(LONG_PRESS_MS, { swallow: true });
  let touch: { x: number; y: number } | null = null;
  let chipEls = $state<Record<string, HTMLElement | undefined>>({});

  $effect(() => {
    if (phone && phoneCol) chipEls[phoneCol.status]?.scrollIntoView({ block: 'nearest', inline: 'center', behavior: 'smooth' });
  });

  function onPhoneDown(e: PointerEvent, task: Task) {
    if ((e.target as HTMLElement).closest('button')) return;
    press.cancel();
    const id = task.id;
    press.start(e.clientX, e.clientY, () => {
      globalThis.navigator?.vibrate?.(10);
      sheetId = id;
    });
  }

  function onTouchStart(e: TouchEvent) {
    const p = e.touches[0];
    touch = e.touches.length === 1 && p ? { x: p.clientX, y: p.clientY } : null;
  }

  function onTouchEnd(e: TouchEvent) {
    const p = e.changedTouches[0];
    if (!touch || !p || sheetId) return void (touch = null);
    const d = swipeDirection(p.clientX - touch.x, p.clientY - touch.y);
    touch = null;
    if (d) {
      press.cancel();
      colIndex = columnAfterSwipe(colIndex, cols.length, d);
    }
  }

  function moveFromSheet(task: Task, status: TaskStatus) {
    sheetId = null;
    const column = cols.find((c) => c.status === status)?.tasks ?? [];
    move(task, status, edgeRank(column, task, 'bottom'));
  }

  function edgeFromSheet(task: Task, where: Edge) {
    sheetId = null;
    move(task, task.status, edgeRank(sheetColumn, task, where));
  }

  let shown = $state<Record<string, number>>({});
  const windowed = (key: string, tasks: Task[]) => tasks.slice(0, shownCount(shown[key], tasks.length, movingId ? tasks.findIndex((x) => x.id === movingId) : -1));
  let known: Set<string> | null = null;
  $effect(() => {
    const all = app.data.tasks;
    const added = newTaskId(known, all);
    known = new Set(all.map((x) => x.id));
    if (!added || !phone || !phoneCol) return;
    const at = phoneCol.tasks.findIndex((x) => x.id === added);
    if (at < 0) return;
    const key = `p:${phoneCol.status}`;
    untrack(() => (shown = { ...shown, [key]: shownCount(shown[key], phoneCol.tasks.length, at) }));
    tick().then(() => document.querySelector(`.pcol [data-task="${CSS.escape(added)}"]`)?.scrollIntoView({ block: 'nearest' }));
  });
  function more(key: string, total: number) {
    const n = grown(shown[key], total);
    if (n !== shown[key]) shown = { ...shown, [key]: n };
  }

  const rest = (status: TaskStatus) => (cols.find((c) => c.status === status)?.tasks ?? []).filter((x) => x.id !== movingId);

  function colorOf(name: string): string {
    const c = colors.get(name);
    return c ?? fallbackProjectColor(name);
  }

  const colLabel = (s: TaskStatus) => t(`kanban.col.${s}` as Key);
  const errText = (e: unknown) => String((e as Error)?.message ?? e);

  function replace(task: Task) {
    app.data = { ...app.data, tasks: app.data.tasks.map((x) => (x.id === task.id ? task : x)) };
  }

  function links(task: Task, on: boolean) {
    chain = on && (task.dependsOn?.length || linked.has(task.id)) ? chainOf(task.id, app.data.tasks) : null;
  }

  async function move(task: Task, status: TaskStatus, rank?: number, force = false) {
    if (!canDrop(task, status)) return;
    if (!force && moveAction(task.status, status, isSession(task)) === 'start' && blockers(task, ids).length) {
      gate = { task, inPlace: false, rank, drop: true };
      return;
    }
    const { next: tasks, prev } = moveTask(app.data.tasks, task.id, status, undefined, rank);
    if (!prev) return;
    app.data = { ...app.data, tasks };
    live = t('kanban.moved', { n: task.number, col: colLabel(status) });
    const reranked = rank !== undefined && rank !== prev.rank;
    try {
      const action = moveAction(prev.status, status, isSession(task));
      let next: Task | null = null;
      if (action === 'start') next = await app.api.startTask(task.id);
      else if (action === 'reject') next = await app.api.reviewTask(task.id, 'reject');
      if (next && reranked && next.rank !== rank) next = await app.api.patchTask(task.id, { rank });
      else if (!next) next = await app.api.patchTask(task.id, action === 'patch' || action === 'complete' ? { status, ...(reranked ? { rank } : {}) } : { rank });
      replace(next);
      if (action === 'complete') holdSession(task);
    } catch (e) {
      app.data = { ...app.data, tasks: revertTask(app.data.tasks, prev) };
      app.notify(errText(e), 'error');
    }
  }

  async function setQuadrant(task: Task, q: Quadrant) {
    const flags = quadrantFlags(q);
    if (task.urgent === flags.urgent && task.important === flags.important) return;
    const prev = app.data.tasks.find((x) => x.id === task.id) ?? task;
    replace({ ...prev, ...flags });
    live = t('kanban.matrixMoved', { n: task.number, q: t(`kanban.q.${q}` as Key) });
    try {
      replace(await app.api.patchTask(task.id, flags));
    } catch (e) {
      replace(prev);
      app.notify(errText(e), 'error');
    }
  }

  async function organize() {
    if (organizeBusy || orgRunning) return;
    organizeBusy = true;
    try {
      const r = await app.api.organize(project || undefined);
      if (r.started.length) app.data = { ...app.data, tasks: [...app.data.tasks.filter((x) => !r.started.some((s) => s.id === x.id)), ...r.started] };
      app.notify(t('kanban.organizeStarted'));
    } catch (e) {
      if ((e as { status?: number })?.status === 409) app.notify(t('kanban.organizeNothing'));
      else app.notify(errText(e), 'error');
    } finally {
      organizeBusy = false;
    }
  }

  function details(task: Task) {
    if (suppressClick) return;
    app.open('taskDetail', task.id);
  }

  function onMxDragStart(e: DragEvent, task: Task) {
    mxDrag = task.id;
    e.dataTransfer?.setData('text/plain', task.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }

  function onMxDragEnd() {
    mxDrag = null;
    mxOver = null;
  }

  function onMxDrop(e: DragEvent, q: Quadrant) {
    e.preventDefault();
    const id = mxDrag;
    onMxDragEnd();
    const task = app.data.tasks.find((x) => x.id === id);
    if (task) setQuadrant(task, q);
  }

  async function rename(task: Task, title: string) {
    const prev = app.data.tasks.find((x) => x.id === task.id) ?? task;
    replace({ ...prev, title });
    try {
      replace(await app.api.patchTask(task.id, { title }));
    } catch (e) {
      replace(prev);
      app.notify(errText(e), 'error');
    }
  }

  async function start(task: Task, inPlace = false, force = false) {
    if (!force && blockers(task, ids).length) {
      gate = { task, inPlace, drop: false };
      return;
    }
    if (inFlight(`start:${task.id}`)) return;
    try {
      replace(await single(`start:${task.id}`, () => app.api.startTask(task.id, { inPlace })));
    } catch (e) {
      app.notify(errText(e), 'error');
    }
  }

  async function remove(task: Task) {
    const before = app.data.tasks;
    app.data = { ...app.data, tasks: before.filter((x) => x.id !== task.id) };
    try {
      await app.api.deleteTask(task.id);
    } catch (e) {
      app.data = { ...app.data, tasks: before };
      app.notify(errText(e), 'error');
    }
  }

  function startAnyway() {
    const g = gate;
    gate = null;
    if (!g) return;
    if (g.drop) move(g.task, 'in_progress', g.rank, true);
    else start(g.task, g.inPlace, true);
  }

  function holdSession(task: Task) {
    const key = `line:${task.lineId}`;
    if (task.lineId && app.items.some((i) => i.key === key && !isOff(i))) holdAll({ kind: 'items', keys: [key] }, 'hold');
  }

  function open(task: Task) {
    if (suppressClick) return;
    if (isTesting(task.status)) app.open('review', task.id);
    else if (isSession(task) && task.lineId) app.select(`line:${task.lineId}`);
    else if (isRunning(task.status) && (task.lineId || task.termId)) app.select(task.lineId ? `line:${task.lineId}` : `term:${task.termId}`);
  }

  async function refresh() {
    refreshing = true;
    try {
      await app.refresh();
    } catch (e) {
      app.notify(errText(e), 'error');
    } finally {
      refreshing = false;
    }
  }

  function rects() {
    return cols
      .map((c) => ({ status: c.status, el: colEls[c.status] }))
      .filter((c) => c.el)
      .map((c) => {
        const r = c.el!.getBoundingClientRect();
        return { status: c.status, left: r.left, right: r.right };
      });
  }

  function onPointerDown(e: PointerEvent, task: Task) {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return;
    pending = { id: task.id, x: e.clientX, y: e.clientY, el: e.currentTarget as HTMLElement };
  }

  function locate(x: number, y: number): { target: TaskStatus | null; index: number } {
    const target = dropTarget(x, rects());
    const list = target ? listEls[target] : undefined;
    if (!target || !list) return { target, index: 0 };
    const mids = [...list.querySelectorAll<HTMLElement>('[data-task]')]
      .filter((el) => el.dataset.task !== drag?.id)
      .map((el) => {
        const r = el.getBoundingClientRect();
        return r.top + r.height / 2;
      });
    return { target, index: insertIndex(y, mids) };
  }

  function settle(): boolean {
    if (!drag) return false;
    pos = pointer;
    let scrolled = false;
    if (board) {
      const b = board.getBoundingClientRect();
      const sx = edgeSpeed(pointer.x, b.left, b.right);
      const left = board.scrollLeft;
      if (sx) board.scrollLeft += sx;
      scrolled = board.scrollLeft !== left;
    }
    const list = drag.target ? listEls[drag.target] : undefined;
    if (list) {
      const r = list.getBoundingClientRect();
      const sy = edgeSpeed(pointer.y, r.top, r.bottom);
      const top = list.scrollTop;
      if (sy) list.scrollTop += sy;
      scrolled ||= list.scrollTop !== top;
    }
    const at = locate(pointer.x, pointer.y);
    if (at.target !== drag.target || at.index !== drag.index) drag = { ...drag, ...at };
    return scrolled;
  }

  const loop = frameLoop(settle);

  function onPointerMove(e: PointerEvent) {
    press.move(e.clientX, e.clientY);
    if (pending && !drag) {
      if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return;
      const r = pending.el.getBoundingClientRect();
      drag = { id: pending.id, dx: pending.x - r.left, dy: pending.y - r.top, w: r.width, target: null, index: 0 };
      pos = { x: e.clientX, y: e.clientY };
      picked = null;
    }
    if (!drag) return;
    pointer = { x: e.clientX, y: e.clientY };
    loop.arm();
  }

  function onPointerUp() {
    press.cancel();
    pending = null;
    loop.cancel();
    if (!drag) return;
    const { id, target, index } = { ...drag, ...locate(pointer.x, pointer.y) };
    drag = null;
    suppressClick = true;
    setTimeout(() => (suppressClick = false));
    const task = app.data.tasks.find((x) => x.id === id);
    if (task && target) drop(task, target, index);
  }

  function drop(task: Task, target: TaskStatus, index: number) {
    const column = cols.find((c) => c.status === target)?.tasks ?? [];
    move(task, target, rankAt(column, index, task));
  }

  function announce(id: string, target: TaskStatus, index: number) {
    live = t('kanban.position', { pos: index + 1, total: rest(target).length + 1, col: colLabel(target) });
    picked = { id, target, index };
  }

  function focusCard(id: string) {
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-task="${id}"]`)?.focus());
  }

  function onCardKey(e: KeyboardEvent, task: Task) {
    if (picked?.id === task.id) {
      if (e.key === ' ' || e.key === 'Enter') {
        e.preventDefault();
        const { target, index } = picked;
        picked = null;
        drop(task, target, index);
        focusCard(task.id);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        picked = null;
        live = '';
      } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        e.preventDefault();
        const target = shiftStatus(picked.target, e.key === 'ArrowLeft' ? -1 : 1);
        announce(task.id, target, Math.min(picked.index, rest(target).length));
      } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const index = Math.max(0, Math.min(rest(picked.target).length, picked.index + (e.key === 'ArrowUp' ? -1 : 1)));
        announce(task.id, picked.target, index);
      }
      return;
    }
    if (e.key === ' ') {
      e.preventDefault();
      const at = cols.find((c) => c.status === task.status)?.tasks.findIndex((x) => x.id === task.id) ?? 0;
      picked = { id: task.id, target: task.status, index: Math.max(0, at) };
      live = t('kanban.dragHint');
    } else if (e.key === 'Enter') {
      open(task);
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const card = e.currentTarget as HTMLElement;
      const list = [...(card.closest('[role="list"]')?.querySelectorAll<HTMLElement>('[data-task]') ?? [])];
      list[list.indexOf(card) + (e.key === 'ArrowDown' ? 1 : -1)]?.focus();
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      const i = cols.findIndex((c) => c.status === task.status);
      const step = e.key === 'ArrowLeft' ? -1 : 1;
      for (let j = i + step; j >= 0 && j < cols.length; j += step) {
        const first = cols[j].tasks[0];
        if (first && !collapsed[cols[j].status]) return focusCard(first.id);
      }
    }
  }

  function toggle(status: TaskStatus) {
    collapsed = { ...collapsed, [status]: !collapsed[status] };
  }
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={onPointerUp} onpointercancel={onPointerUp} />

{#snippet moreCards(key: string, count: number, total: number)}
  {#if count < total}
    {#key count}<div class="more" aria-hidden="true" style:height="{(total - count) * CARD_EST}px" use:reveal={() => more(key, total)}></div>{/key}
  {/if}
{/snippet}

{#snippet quadrants()}
  <div class="grid" class:phone>
    {#each QUADRANTS as q (q)}
      {@render quadrantPane(q)}
    {/each}
  </div>
  {@render quadrantPane('none')}
{/snippet}

{#snippet quadrantPane(q: Quadrant)}
  {@const qtasks = windowed(`q:${q}`, mx[q])}
  <section class="quad {q}" class:over={mxOver === q && !!mxDrag} aria-label={t(`kanban.q.${q}` as Key)} ondragover={(e) => { if (mxDrag) { e.preventDefault(); mxOver = q; } }} ondragleave={() => (mxOver = mxOver === q ? null : mxOver)} ondrop={(e) => onMxDrop(e, q)}>
    <header>
      <span class="qmark"></span>
      <h3>{t(`kanban.q.${q}` as Key)}</h3>
      <span class="count">{mx[q].length}</span>
      <span class="qhint faint">{t(`kanban.q.${q}.hint` as Key)}</span>
    </header>
    <div class="qlist" role="list">
      {#each qtasks as task (task.id)}
        <div class="qitem" role="presentation" draggable={!phone} ondragstart={(e) => onMxDragStart(e, task)} ondragend={onMxDragEnd}>
          <TaskCard
            {task}
            color={colorOf(task.project)}
            status={cardStatus(task, next, ids)}
            filtered={!!project}
            showStatus
            showQuadrant={false}
            lifted={sheetId === task.id}
            onqueue={() => move(task, 'auto')}
            onstart={(inPlace) => start(task, inPlace)}
            ondelete={() => remove(task)}
            chained={!!chain?.has(task.id)}
            onlinks={(on) => links(task, on)}
            onchain={() => app.open('task', task.id)}
            onopen={() => open(task)}
            ondetails={() => details(task)}
            onmore={phone ? () => (sheetId = task.id) : undefined}
            onpointerdown={phone ? (e) => onPhoneDown(e, task) : undefined}
          />
        </div>
      {:else}
        <div class="qempty faint">{t('kanban.matrixEmpty')}</div>
      {/each}
      {@render moreCards(`q:${q}`, qtasks.length, mx[q].length)}
    </div>
  </section>
{/snippet}

{#if phone}
  <div class="kanban phone-board">
    <header class="pbar">
      <h1>{t('kanban.title')}</h1>
      <span class="spacer"></span>
      <button class="icon-btn touch" onclick={() => app.open('project')} title={t('kanban.createProject')} aria-label={t('kanban.createProject')}><FolderPlus size={18} /></button>
      <button class="icon-btn touch" onclick={refresh} title={t('common.refresh')} aria-label={t('common.refresh')}>
        <RefreshCw size={18} class={refreshing ? 'spin' : ''} />
      </button>
      <button class="btn primary touch" onclick={() => app.open('task')}><Plus size={16} />{t('kanban.addTask')}</button>
    </header>
    <div class="pfilters">
      <label class="search">
        <Search size={14} />
        <input type="search" placeholder={t('kanban.search')} aria-label={t('kanban.search')} bind:value={search} />
      </label>
      <select bind:value={project} aria-label={t('task.project')}>
        <option value="">{t('kanban.allProjects')}</option>
        {#each projectPaths as path (path)}<option value={path}>{projectName(app.allProjects, path)}</option>{/each}
      </select>
      <button class="icon-btn touch toggle" class:on={hideSession} aria-pressed={hideSession} title={t('kanban.hideSessions')} aria-label={t('kanban.hideSessions')} onclick={() => saveSettings({ hideSessionTasks: !hideSession })}><Radio size={18} /></button>
    </div>
    <div class="pview">
      <Segmented label={t('kanban.view')} value={view} options={viewOptions} onchange={(v) => saveSettings({ kanbanView: v })} />
      <button class="btn touch" disabled={organizeBusy || orgRunning} title={t('kanban.organizeHint')} onclick={organize}>
        {#if orgRunning}<LoaderCircle size={15} class="spin" />{t('kanban.organizing')}{:else}<Sparkles size={15} />{t('kanban.organize')}{/if}
      </button>
    </div>
    {#if view === 'matrix'}
      <div class="pmatrix">{@render quadrants()}</div>
    {:else}
    <div class="chips" role="tablist" aria-label={t('mobile.tasks.columns')}>
      {#each cols as col, i (col.status)}
        <button class="chipbtn {col.status}" role="tab" aria-selected={i === colIndex} class:on={i === colIndex} bind:this={chipEls[col.status]} onclick={() => (colIndex = i)}>
          <span class="cbar"></span>{colLabel(col.status)}<span class="count">{col.tasks.length}</span>
        </button>
      {/each}
    </div>
    {#if phoneCol}
      {@const ptasks = windowed(`p:${phoneCol.status}`, phoneCol.tasks)}
      <div class="pcol" role="tabpanel" tabindex="-1" aria-label={colLabel(phoneCol.status)} ontouchstart={onTouchStart} ontouchend={onTouchEnd} ontouchcancel={() => (touch = null)}>
        {#if isQueued(phoneCol.status)}
          <div class="pextra">
            <span class="chip">{t('kanban.autoChip', { running: summary.running, queued: summary.queued })}</span>
            <span class="pill mono">{settings.defaultModel || t('model.default')}</span>
          </div>
        {/if}
        <div class="plist" role="list">
          {#each ptasks as task (task.id)}
            <TaskCard
              {task}
              color={colorOf(task.project)}
              status={cardStatus(task, next, ids)}
              filtered={!!project}
              lifted={sheetId === task.id}
              onqueue={() => move(task, 'auto')}
              onstart={(inPlace) => start(task, inPlace)}
              ondelete={() => remove(task)}
              chained={!!chain?.has(task.id)}
              onlinks={(on) => links(task, on)}
              onchain={() => app.open('task', task.id)}
              onopen={() => open(task)}
              ondetails={() => details(task)}
              onmore={() => (sheetId = task.id)}
              onpointerdown={(e) => onPhoneDown(e, task)}
            />
          {:else}
            <div class="pempty faint">{t('kanban.empty')}</div>
          {/each}
          {@render moreCards(`p:${phoneCol.status}`, ptasks.length, phoneCol.tasks.length)}
          {#if phoneCol.tasks.length}<p class="phint faint">{t('mobile.tasks.longPress')}</p>{/if}
        </div>
      </div>
    {/if}
    {/if}
    <div class="sr-only" aria-live="assertive">{live}</div>
  </div>
  {#if sheetTask}
    {@const st = sheetTask}
    <Modal title={t('mobile.tasks.moveTo', { n: st.number })} onclose={() => (sheetId = null)} width={420}>
      <div class="sheet">
        <div class="stitle">#{st.number} {st.title}</div>
        {#if isRunning(st.status) || isTesting(st.status) || (isSession(st) && st.lineId)}
          <button class="srow" onclick={() => { sheetId = null; open(st); }}><ArrowRight size={16} />{t('mobile.tasks.open')}</button>
        {/if}
        {#each edgeMoves(sheetColumn, st) as where (where)}
          <button class="srow" onclick={() => edgeFromSheet(st, where)}>
            {#if where === 'top'}<ArrowUpToLine size={16} />{:else}<ArrowDownToLine size={16} />{/if}
            {t(where === 'top' ? 'mobile.tasks.top' : 'mobile.tasks.bottom', { col: colLabel(st.status) })}
          </button>
        {/each}
        {#if !isSession(st)}
          <button class="srow" onclick={() => { sheetId = null; details(st); }}><MessageSquare size={16} />{t('kanban.details')}</button>
        {/if}
        {#each moveChoices(st.status, isSession(st)) as status (status)}
          <button class="srow" onclick={() => moveFromSheet(st, status)}><span class="sdot {status}"></span>{colLabel(status)}</button>
        {/each}
        {#if !isSession(st)}
          <div class="shead">{t('mobile.tasks.priority')}</div>
          {#each [...QUADRANTS, 'none' as const] as q (q)}
            <button class="srow" onclick={() => { sheetId = null; setQuadrant(st, q); }}><span class="qdot {q}"></span>{t(`kanban.q.${q}` as Key)}</button>
          {/each}
        {/if}
      </div>
    </Modal>
  {/if}
{:else}
<div class="kanban" class:dragging={!!drag}>
  <header class="bar">
    <button class="btn ghost back" onclick={() => (app.view = 'agents')}>
      <ArrowLeft size={14} />{t('kanban.backToAgents')}
    </button>
    <h1>{t('kanban.title')}</h1>
    <div class="tools">
      <label class="search">
        <Search size={13} />
        <input type="search" placeholder={t('kanban.search')} aria-label={t('kanban.search')} bind:value={search} />
      </label>
      <select bind:value={project} aria-label={t('task.project')}>
        <option value="">{t('kanban.allProjects')}</option>
        {#each projectPaths as path (path)}<option value={path}>{projectName(app.allProjects, path)}</option>{/each}
      </select>
      <button class="btn toggle" class:on={hideSession} aria-pressed={hideSession} title={t('kanban.hideSessionsHint')} onclick={() => saveSettings({ hideSessionTasks: !hideSession })}>
        <Radio size={14} />{t('kanban.hideSessions')}
      </button>
      <Segmented label={t('kanban.view')} value={view} options={viewOptions} onchange={(v) => saveSettings({ kanbanView: v })} />
      <button class="btn" disabled={organizeBusy || orgRunning} title={t('kanban.organizeHint')} onclick={organize}>
        {#if orgRunning}<LoaderCircle size={14} class="spin" />{t('kanban.organizing')}{:else}<Sparkles size={14} />{t('kanban.organize')}{/if}
      </button>
      <button class="btn" onclick={() => app.open('project')}><FolderPlus size={14} />{t('kanban.createProject')}</button>
      <button class="icon-btn" onclick={refresh} title={t('common.refresh')} aria-label={t('common.refresh')}>
        <RefreshCw size={14} class={refreshing ? 'spin' : ''} />
      </button>
      <button class="btn primary" onclick={() => app.open('task')}><Plus size={14} />{t('kanban.addTask')}</button>
    </div>
  </header>

  {#if view === 'matrix'}
    <div class="matrix">{@render quadrants()}</div>
  {:else}
  <div class="board" bind:this={board}>
    {#each cols as col (col.status)}
      <KanbanColumn
        status={col.status}
        count={col.tasks.length}
        collapsible={isDone(col.status)}
        collapsed={!!collapsed[col.status] && dropOn !== col.status}
        target={dropOn === col.status && origin !== col.status}
        empty={!rest(col.status).length}
        bind:el={colEls[col.status]}
        bind:list={listEls[col.status]}
        ontoggle={() => toggle(col.status)}
      >
        {#snippet extra()}
          {#if isQueued(col.status)}
            <span class="chip">{t('kanban.autoChip', { running: summary.running, queued: summary.queued })}</span>
            <span class="pill mono" title={`${t('kanban.defaultModel')} · ${agentName(app.agents, settings.defaultAgent)}`}>{settings.defaultModel || t('model.default')}</span>
          {/if}
        {/snippet}
        {@const ctasks = windowed(col.status, col.tasks)}
        {#each ctasks as task (task.id)}
          {#if dropAt?.status === col.status && task.id === before}<div class="insert" aria-hidden="true"></div>{/if}
          <TaskCard
            {task}
            color={colorOf(task.project)}
            status={cardStatus(task, next, ids)}
            filtered={!!project}
            dragging={drag?.id === task.id}
            lifted={picked?.id === task.id}
            onqueue={() => move(task, 'auto')}
            onstart={(inPlace) => start(task, inPlace)}
            ondelete={() => remove(task)}
            chained={!!chain?.has(task.id)}
            onlinks={(on) => links(task, on)}
            onchain={() => app.open('task', task.id)}
            onopen={() => open(task)}
            ondetails={() => details(task)}
            onrename={(title) => rename(task, title)}
            onpointerdown={(e) => onPointerDown(e, task)}
            onkeydown={(e) => onCardKey(e, task)}
          />
        {/each}
        {#if dropAt?.status === col.status && before === null && rest(col.status).length}<div class="insert" aria-hidden="true"></div>{/if}
        {@render moreCards(col.status, ctasks.length, col.tasks.length)}
      </KanbanColumn>
    {/each}
  </div>
  {/if}

  {#if drag && dragTask}
    <div class="drag-ghost" style:transform="translate3d({pos.x - drag.dx}px, {pos.y - drag.dy}px, 0)" style:width="{drag.w}px">
      <TaskCard task={dragTask} color={colorOf(dragTask.project)} status={cardStatus(dragTask, next, ids)} filtered={!!project} ghost />
    </div>
  {/if}

  <div class="sr-only" aria-live="assertive">{live}</div>
</div>
{/if}

{#if gate}
  {@const g = gate}
  <Modal title={t('kanban.startBlockedTitle', { n: g.task.number })} onclose={() => (gate = null)} width={420}>
    <p class="gate">{t('kanban.startBlockedBody', { n: g.task.number, list: blockers(g.task, ids).map((d) => `#${d.number}`).join(', ') })}</p>
    {#snippet footer()}
      <button class="btn ghost" onclick={() => (gate = null)}>{t('common.cancel')}</button>
      <button class="btn primary" onclick={startAnyway}>{t('kanban.startAnyway')}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .more {
    flex: none;
  }
  .gate {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .kanban {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .kanban.dragging {
    cursor: grabbing;
    user-select: none;
    -webkit-user-select: none;
  }
  .kanban.dragging .board :global(.card) {
    pointer-events: none;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  .back {
    color: var(--text-2);
    padding: 0 8px 0 6px;
  }
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .tools {
    margin-left: auto;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    color: var(--text-3);
  }
  .search :global(svg) {
    position: absolute;
    left: 9px;
    pointer-events: none;
  }
  .search input {
    width: 220px;
    height: 28px;
    padding-left: 28px;
  }
  select {
    height: 28px;
    padding: 0 8px;
  }
  .board {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 10px;
    padding: 14px 16px 16px;
    overflow-x: auto;
  }
  .chip {
    min-width: 0;
    font-size: 11px;
    color: var(--text-2);
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--surface-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pill {
    flex: none;
    white-space: nowrap;
    font-size: 10.5px;
    color: var(--text-2);
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    line-height: 16px;
  }
  .toggle.on {
    color: var(--text);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .insert {
    flex: none;
    height: 2px;
    margin: -5px 2px;
    border-radius: 1px;
    background: var(--accent);
  }
  .drag-ghost {
    position: fixed;
    left: 0;
    top: 0;
    will-change: transform;
    z-index: var(--z-drag);
    pointer-events: none;
  }
  .phone-board {
    min-height: 0;
  }
  .pbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px 4px 16px;
  }
  .pbar h1 {
    font-size: 17px;
  }
  .spacer {
    flex: 1;
  }
  .touch {
    min-width: 44px;
    min-height: 44px;
  }
  .pfilters {
    display: flex;
    gap: 8px;
    padding: 4px 12px 8px;
  }
  .pfilters .search {
    flex: 1;
    min-width: 0;
  }
  .pfilters .search input {
    width: 100%;
    height: 40px;
    font-size: 15px;
  }
  .pfilters select {
    height: 40px;
    max-width: 40%;
    font-size: 15px;
  }
  .chips {
    flex: none;
    display: flex;
    gap: 6px;
    padding: 0 12px 8px;
    overflow-x: auto;
    scrollbar-width: none;
    border-bottom: 1px solid var(--border);
  }
  .chips::-webkit-scrollbar {
    display: none;
  }
  .chipbtn {
    --c: var(--text-3);
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 44px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-2);
    font-size: 14px;
    white-space: nowrap;
    background: var(--surface);
  }
  .chipbtn.on {
    color: var(--text);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .chipbtn.auto,
  .sdot.auto {
    --c: var(--st-working);
  }
  .chipbtn.in_progress,
  .sdot.in_progress {
    --c: var(--st-input);
  }
  .chipbtn.in_testing,
  .sdot.in_testing {
    --c: var(--st-testing);
  }
  .chipbtn.merged,
  .sdot.merged {
    --c: var(--st-input);
  }
  .chipbtn.completed,
  .sdot.completed {
    --c: var(--st-done);
  }
  .cbar,
  .sdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c, var(--text-3));
    flex: none;
  }
  .chipbtn .count {
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .pcol {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    touch-action: pan-y;
  }
  .pextra {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px 0;
  }
  .plist {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 12px calc(16px + env(safe-area-inset-bottom));
  }
  .pempty {
    padding: 40px 0;
    text-align: center;
  }
  .phint {
    margin: 4px 0 0;
    text-align: center;
    font-size: 12px;
  }
  .sheet {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: env(safe-area-inset-bottom);
  }
  .stitle {
    padding: 0 4px 8px;
    color: var(--text-2);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .srow {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 48px;
    padding: 0 8px;
    border-radius: var(--radius);
    font-size: 15px;
    color: var(--text);
    text-align: left;
  }
  .srow:active {
    background: var(--surface-active);
  }
  .sdot {
    margin: 0 4px;
  }
  .pview {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 8px;
  }
  .pview :global(.seg) {
    flex: 1;
  }
  .pview :global(.seg button) {
    height: 36px;
    font-size: 14px;
  }
  .pmatrix {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px calc(16px + env(safe-area-inset-bottom));
    border-top: 1px solid var(--border);
  }
  .matrix {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px 16px;
  }
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
    gap: 10px;
  }
  .grid.phone {
    flex: none;
    grid-template-columns: 1fr;
    grid-template-rows: none;
  }
  .quad {
    --c: var(--text-3);
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
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
  .quad.none {
    flex: none;
    max-height: 190px;
  }
  .quad.over {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .quad header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px 6px;
    min-width: 0;
  }
  .quad .qmark {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
    flex: none;
  }
  .quad h3 {
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
  }
  .quad .count {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .qhint {
    margin-left: auto;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .qlist {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 2px 10px 10px;
  }
  .quad.none .qlist {
    flex-direction: row;
    flex-wrap: nowrap;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .quad.none .qitem {
    flex: 0 0 280px;
  }
  :global(.phone) .quad.none .qlist {
    flex-direction: column;
    overflow-x: hidden;
    overflow-y: visible;
  }
  :global(.phone) .quad.none .qitem {
    flex: none;
  }
  :global(.phone) .quad.none {
    max-height: none;
  }
  .qitem {
    min-width: 0;
  }
  .qitem[draggable='true'] {
    cursor: grab;
  }
  .qempty {
    margin: auto;
    padding: 12px 0;
    font-size: 12px;
  }
  .shead {
    padding: 10px 8px 2px;
    font-size: 12px;
    color: var(--text-3);
  }
  .qdot {
    width: 8px;
    height: 8px;
    margin: 0 4px;
    border-radius: 50%;
    flex: none;
    background: var(--st-failed);
  }
  .qdot.plan {
    background: var(--st-testing);
  }
  .qdot.delegate {
    background: var(--st-input);
  }
  .qdot.drop,
  .qdot.none {
    background: var(--text-3);
  }
  @media (max-width: 1100px) {
    .search input {
      width: 150px;
    }
  }
</style>
