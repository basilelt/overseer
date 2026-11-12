<script lang="ts">
  import Bot from '@lucide/svelte/icons/bot';
  import Send from '@lucide/svelte/icons/send';
  import Modal from '../ui/Modal.svelte';
  import { app, projectName } from '../../state.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { flagPatch, isSession, mergeMessage, quadrant, quadrantFlags } from '../../kanban/kanban';
  import { onDestroy, untrack } from 'svelte';
  import DepPicker from './DepPicker.svelte';
  import QuorumChips from './QuorumChips.svelte';
  import { agentName } from '../../agents';
  import { fromOverseer } from '../../kanban/quorum';
  import type { Task, TaskMessage } from '../../types';
  import { isSubmitChord } from '../../chord';
  import { isRunning } from '../../kanban/task-status';

  let { onclose }: { onclose: () => void } = $props();

  const MAX = 8000;

  const task = $derived(app.data.tasks.find((x) => x.id === app.overlayArg));
  const parent = $derived(task?.parentId ? app.data.tasks.find((x) => x.id === task.parentId) : undefined);
  const quad = $derived(task ? quadrant(task) : 'none');
  const live = $derived(!!task && isRunning(task.status) && !!task.termId);
  const errText = (e: unknown) => String((e as Error)?.message ?? e);

  let messages = $state<TaskMessage[]>([]);
  let loaded = $state(false);
  let draft = $state('');
  let sending = $state(false);
  let list = $state<HTMLElement>();

  async function load(id: string) {
    try {
      const got = await app.api.taskMessages(id);
      messages = got.reduce((acc, m) => mergeMessage(acc, m), messages);
    } catch (e) {
      app.notify(errText(e), 'error');
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    const id = task?.id;
    if (id) untrack(() => load(id));
  });

  const off = app.onTaskMessage((m) => {
    if (task && (m.taskId === task.id || m.from === task.id)) messages = mergeMessage(messages, m);
  });
  onDestroy(off);

  $effect(() => {
    messages.length;
    if (list) list.scrollTop = list.scrollHeight;
  });

  function replace(next: Task) {
    app.data = { ...app.data, tasks: app.data.tasks.map((x) => (x.id === next.id ? next : x)) };
  }

  async function patch(change: Partial<Task>) {
    if (!task) return;
    const prev = task;
    replace({ ...prev, ...change });
    try {
      replace(await app.api.patchTask(prev.id, change));
    } catch (e) {
      replace(prev);
      app.notify(errText(e), 'error');
    }
  }

  async function send() {
    const body = draft.trim();
    if (!task || !body || sending) return;
    sending = true;
    try {
      const m = await app.api.sendTaskMessage(task.id, body);
      messages = mergeMessage(messages, m);
      draft = '';
    } catch (e) {
      app.notify(errText(e), 'error');
    } finally {
      sending = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (isSubmitChord(e)) {
      e.preventDefault();
      send();
    }
  }

  function sender(m: TaskMessage): string {
    if (fromOverseer(m)) return t('task.fromOverseer');
    if (m.from === null) return t('task.you');
    if (m.from === task?.id) {
      const to = app.data.tasks.find((x) => x.id === m.taskId);
      return to ? t('task.toTask', { n: to.number }) : t('task.messages');
    }
    return m.fromNumber ? `#${m.fromNumber}${m.fromTitle ? ` ${m.fromTitle}` : ''}` : (m.fromTitle ?? '');
  }
</script>

<Modal title={task ? t('task.detail', { n: task.number }) : t('common.error')} {onclose} width={620}>
  {#if task}
    <div class="body">
      <div class="head">
        <div class="title">{task.title}</div>
        <div class="meta">
          <span>{projectName(app.allProjects, task.project)}</span>
          <span class="stat {task.status}">{t(`kanban.col.${task.status}` as Key)}</span>
          {#if task.origin === 'agent'}
            <span class="by"><Bot size={11} />{parent ? t('task.createdByTask', { n: parent.number }) : t('task.createdByAgent')}</span>
          {/if}
          <QuorumChips {task} />
        </div>
        {#if task.description}<p class="desc">{task.description}</p>{/if}
      </div>

      {#if task.quorum}
        <section>
          <h4>{t('quorum.config')}</h4>
          <p class="desc">{t('quorum.configBody', { workers: task.quorum.workers.map((id) => agentName(app.agents, id)).join(', '), reviewers: task.quorum.reviewers.map((id) => agentName(app.agents, id)).join(', ') })}</p>
        </section>
      {/if}

      {#if task.votes}
        <section>
          <h4>{t('quorum.votes')} · {t('quorum.round', { n: task.votes.round })}</h4>
          <ul class="votes">
            {#each task.votes.votes as v (v.from)}
              <li class:no={!v.approve}>
                <div class="who"><span>{v.role === 'orchestrator' ? t('quorum.orchestrator') : t('quorum.reviewer')} #{v.number} · {agentName(app.agents, v.agent)}</span><span class="verdict">{v.approve ? t('quorum.approve') : t('quorum.reject')}</span></div>
                {#if v.reason}<div class="reason">{v.reason}</div>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if !isSession(task)}
        <section>
          <h4>{t('task.priority')}</h4>
          <div class="prio">
            <label class="tog">
              <input type="checkbox" class="switch" checked={!!task.urgent} onchange={(e) => patch(flagPatch(task, 'urgent', e.currentTarget.checked))} />
              <span>{t('task.urgent')}</span>
            </label>
            <label class="tog">
              <input type="checkbox" class="switch" checked={!!task.important} onchange={(e) => patch(flagPatch(task, 'important', e.currentTarget.checked))} />
              <span>{t('task.important')}</span>
            </label>
            <span class="quad {quad}">{t(`kanban.q.${quad}` as Key)}</span>
            {#if quad !== 'none'}
              <button class="btn ghost small" onclick={() => patch(quadrantFlags('none'))}>{t('task.clearPriority')}</button>
            {/if}
          </div>
        </section>
      {/if}

      {#if !isSession(task)}
        <section>
          <h4>{t('task.dependsOn')}</h4>
          <DepPicker value={task.dependsOn ?? []} project={task.project} self={task.id} inherit={task.inheritBranch} onchange={(ids) => patch({ dependsOn: ids })} oninherit={(on) => patch({ inheritBranch: on })} />
        </section>
      {/if}

      <section class="thread">
        <h4>{t('task.messages')}</h4>
        <div class="list" bind:this={list} role="log" aria-label={t('task.messages')}>
          {#each messages as m (m.id)}
            <div class="msg" class:mine={m.from === null && !fromOverseer(m)} class:out={m.from === task.id}>
              <div class="who"><span>{sender(m)}</span>{#if !m.delivered}<span class="faint">{t('task.undelivered')}</span>{/if}</div>
              <div class="text">{m.body}</div>
            </div>
          {:else}
            <div class="empty faint">{loaded ? t('task.noMessages') : ''}</div>
          {/each}
        </div>
        <div class="compose">
          <textarea rows="2" maxlength={MAX} bind:value={draft} onkeydown={onKey} placeholder={t('task.messagePlaceholder')} aria-label={t('task.messagePlaceholder')}></textarea>
          <button class="btn primary" disabled={sending || !draft.trim()} onclick={send}><Send size={13} />{t('task.send')}</button>
        </div>
        {#if !live}<p class="hint faint">{t('task.sendLater')}</p>{/if}
      </section>
    </div>
  {/if}
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  .title {
    font-size: 15px;
    font-weight: 600;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .desc {
    margin: 10px 0 0;
    font-size: 13px;
    color: var(--text-2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 120px;
    overflow-y: auto;
  }
  h4 {
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .stat,
  .by,
  .quad {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 11px;
    line-height: 18px;
    white-space: nowrap;
  }
  .stat {
    background: var(--surface-2);
  }
  .by {
    border: 1px solid var(--border);
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
  .votes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .votes li {
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-left: 3px solid var(--st-done);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .votes li.no {
    border-left-color: var(--st-failed);
  }
  .votes .who {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    color: var(--text-2);
  }
  .votes .reason {
    margin-top: 2px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .prio {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 18px;
  }
  .tog {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 80px;
    max-height: 260px;
    overflow-y: auto;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .msg {
    align-self: flex-start;
    max-width: 88%;
    padding: 6px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .msg.mine,
  .msg.out {
    align-self: flex-end;
  }
  .msg.mine {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .who {
    display: flex;
    gap: 8px;
    margin-bottom: 2px;
    font-size: 11px;
    color: var(--text-2);
  }
  .text {
    font-size: 13px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .empty {
    margin: auto;
    font-size: 12px;
  }
  .compose {
    display: flex;
    gap: 8px;
    align-items: flex-end;
    margin-top: 8px;
  }
  .compose textarea {
    flex: 1;
    min-width: 0;
    resize: vertical;
    min-height: 44px;
    max-height: 140px;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 12px;
  }
</style>
