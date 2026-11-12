<script lang="ts">
  import { onMount } from 'svelte';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import X from '@lucide/svelte/icons/x';
  import Modal from '../ui/Modal.svelte';
  import AgentPicker from '../ui/AgentPicker.svelte';
  import ModelField from '../ui/ModelField.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import { app } from '../../state.svelte';
  import { settings } from '../../settings/settings.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { findAgent, hasModel, pickAgent, taskAgents } from '../../agents';
  import { initialChoice, resolveProfile, TASK_TYPES } from '../../profiles/profiles';
  import ProfileSelect from '../ui/ProfileSelect.svelte';
  import DepPicker from './DepPicker.svelte';
  import QuorumField from './QuorumField.svelte';
  import { defaultQuorum, quorumBody } from '../../kanban/quorum';
  import type { Effort, TaskType } from '../../types';
  import { taskDrafts, type TaskDraft } from '../../kanban/task-drafts';
  import { ft } from '../../feedback/feedback.svelte';
  import { dropZone } from '../../drop';
  import { uploadBlobs, uploadPaths, type Attachment, type Failure } from '../../kanban/task-attachments';
  import { isSubmitChord } from '../../chord';
  import { uploadFailures } from '../../upload-pipeline';

  let { onclose }: { onclose: () => void } = $props();

  const projects = $derived(app.allProjects);
  const agents = $derived(taskAgents(app.agents));

  const initialAgent = pickAgent(app.agents, settings.defaultAgent);
  const after = app.data.tasks.find((x) => x.id === app.overlayArg);
  const initialProject = after?.project ?? app.projectFor(app.currentWorkspace?.path ?? undefined) ?? app.allProjects[0]?.path ?? '';
  const fallback = (id: string) => ({ model: id === settings.defaultAgent ? settings.defaultModel : '', effort: settings.defaultEffort });
  const initial = initialChoice(resolveProfile(app.profiles, { cwd: initialProject }), findAgent(app.agents, initialAgent), fallback(initialAgent));

  const draftKey = after ? `after:${after.id}` : 'new';
  const saved = taskDrafts.get(draftKey);
  let title = $state(saved?.title ?? '');
  let description = $state(saved?.description ?? '');
  let project = $state(initialProject);
  let profile = $state('');
  let taskType = $state<TaskType | ''>('');
  let agent = $state(initialAgent);
  let model = $state(initial.model);
  let effort = $state<Effort | undefined>(initial.effort);
  let ticketKey = $state(saved?.ticketKey ?? '');
  let dependsOn = $state<string[]>(after ? [after.id] : []);
  let inheritBranch = $state<boolean | null>(null);
  let quorumOn = $state(false);
  let quorum = $state(defaultQuorum(app.agents, initialAgent));
  let error = $state('');
  let busy = $state(false);
  let importing = $state(false);
  let attachments = $state<Attachment[]>(saved?.attachments ?? []);
  let uploading = $state(0);
  let dropping = $state(false);
  let zone = $state<HTMLElement | null>(null);
  let picker = $state<HTMLInputElement | null>(null);

  const info = $derived(findAgent(app.agents, agent));

  $effect(() => {
    const d: TaskDraft = { title, description, ticketKey, attachments };
    taskDrafts.set(draftKey, d);
  });
  const effortOptions = $derived(info?.efforts ?? []);

  function reset() {
    ({ model, effort } = initialChoice(resolveProfile(app.profiles, { explicit: profile, taskType: taskType || undefined, cwd: project }), findAgent(app.agents, agent), fallback(agent)));
  }

  function pick(fill: { title: string; description: string; ticketKey: string; taskType?: TaskType }) {
    ({ title, description, ticketKey } = fill);
    if (fill.taskType && app.profiles?.taskTypes) setTaskType(fill.taskType);
    error = '';
    importing = false;
  }

  function setAgent(id: string) {
    agent = id;
    quorum = defaultQuorum(app.agents, id);
    reset();
  }

  function setTaskType(ty: TaskType | '') {
    taskType = ty;
    if (!profile) reset();
  }

  function setProfile(id: string) {
    profile = id;
    reset();
  }

  async function attach(run: (c: NonNullable<typeof app.client>, fail: (f: Failure) => void) => Promise<Attachment[]>) {
    const client = app.client;
    if (!client || !app.connected) return app.notify(t('ports.disconnected'), 'error');
    uploading++;
    const failures = uploadFailures();
    try {
      attachments = [...attachments, ...(await run(client, (f) => failures.fail(f.name, f.error)))];
    } finally {
      uploading--;
    }
    const failure = failures.summary();
    if (failure) app.notify(failure, 'error');
  }

  const addFiles = (files: File[]) => files.length && attach((c, fail) => uploadBlobs(c, files, fail));
  const addPaths = (paths: string[]) => paths.length && attach((c, fail) => uploadPaths(c, paths, fail));

  function onPaste(e: ClipboardEvent) {
    const files = Array.from(e.clipboardData?.files ?? []);
    if (!files.length) return;
    e.preventDefault();
    void addFiles(files);
  }

  function onDomDrop(e: DragEvent) {
    dropping = false;
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (!files.length) return;
    e.preventDefault();
    void addFiles(files);
  }

  onMount(() => dropZone({ el: () => zone?.closest('.modal'), rank: 3, hover: (on) => (dropping = on), drop: (paths) => void addPaths(paths) }));

  async function submit() {
    if (busy || uploading) return;
    if (!title.trim()) {
      error = t('task.titleRequired');
      return;
    }
    busy = true;
    try {
      const task = await app.api.createTask({
        title: title.trim(),
        description: description.trim(),
        project,
        agent,
        model: hasModel(info) ? model : '',
        effort: effortOptions.length ? effort : undefined,
        ticketKey: ticketKey.trim() || undefined,
        ...(profile ? { profile } : {}),
        ...(taskType ? { taskType } : {}),
        ...(dependsOn.length ? { dependsOn } : {}),
        ...(dependsOn.length === 1 && inheritBranch !== null ? { inheritBranch } : {}),
        ...quorumBody(quorumOn, agent, quorum),
        ...(attachments.length ? { attachments: attachments.map(({ path, name }) => ({ path, name })) } : {}),
      });
      if (!app.data.tasks.some((x) => x.id === task.id)) app.data = { ...app.data, tasks: [...app.data.tasks, task] };
      taskDrafts.delete(draftKey);
      onclose();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (isSubmitChord(e)) {
      e.preventDefault();
      submit();
    }
  }

  function autofocus(node: HTMLElement) {
    node.focus();
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal title={t('task.new')} {onclose} width={540}>
  <form
    class="form"
    bind:this={zone}
    onpaste={onPaste}
    ondragover={(e) => (e.preventDefault(), (dropping = true))}
    ondragleave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && (dropping = false)}
    ondrop={onDomDrop}
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <div class="field">
      <span class="head"><label for="task-title">{t('task.title')}</label><button type="button" class="link" aria-expanded={importing} onclick={() => (importing = !importing)}>{t('task.import')}</button></span>
      <input
        id="task-title"
        use:autofocus
        bind:value={title}
        placeholder={t('task.titlePlaceholder')}
        aria-invalid={!!error}
        oninput={() => (error = '')}
      />
      {#if error}<em class="err">{error}</em>{/if}
    </div>
    {#if importing && project}
      {#await import('./SourcePicker.svelte') then { default: SourcePicker }}
        <SourcePicker {project} onpick={pick} />
      {/await}
    {/if}
    <label>
      <span>{t('task.description')}</span>
      <textarea rows="4" bind:value={description} placeholder={t('task.descriptionPlaceholder')} spellcheck="true"></textarea>
    </label>
    <div class="field">
      <span>{ft('attachments')}</span>
      <div class="drop" class:on={dropping}>
        <Paperclip size={14} />
        <span class="faint">{ft('attachHint')}</span>
        <button type="button" class="link" onclick={() => picker?.click()}>{ft('browse')}</button>
        {#if uploading}<span class="faint">· {uploading}…</span>{/if}
        <input bind:this={picker} type="file" multiple hidden onchange={(e) => (void addFiles(Array.from(e.currentTarget.files ?? [])), (e.currentTarget.value = ''))} />
      </div>
      {#if attachments.length}
        <ul class="files">
          {#each attachments as a, i (a.path)}
            <li>
              <span class="mono" title={a.name}>{a.name}</span>
              <button type="button" class="icon-btn" aria-label={ft('remove', { name: a.name })} title={ft('remove', { name: a.name })} onclick={() => (attachments = attachments.filter((_, j) => j !== i))}><X size={12} /></button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
    <div class="row">
      <label>
        <span>{t('task.project')}</span>
        <select bind:value={project} onchange={() => !profile && reset()}>
          {#each projects as p (p.path)}<option value={p.path}>{p.name}</option>{/each}
        </select>
      </label>
      <label>
        <span>{t('task.ticketKey')}</span>
        <input bind:value={ticketKey} placeholder="PROJ-123" class="mono" />
      </label>
    </div>
    <div class="row">
      <div class="field">
        <span>{t('task.agent')}</span>
        <AgentPicker {agents} value={agent} label={t('task.agent')} onchange={setAgent} />
      </div>
      {#if hasModel(info)}
        <div class="field">
          <span>{t('task.model')}</span>
          <ModelField agent={info} value={model} label={t('task.model')} onchange={(v) => (model = v)} />
        </div>
      {/if}
    </div>
    <div class="field">
      <span>{t('task.dependsOn')}</span>
      <DepPicker value={dependsOn} {project} inherit={inheritBranch} onchange={(ids) => (dependsOn = ids)} oninherit={(on) => (inheritBranch = on)} />
    </div>
    {#if app.profiles?.taskTypes}
      <div class="row">
        <label>
          <span>{t('task.type')}</span>
          <select data-tour="task-type" value={taskType} onchange={(e) => setTaskType(e.currentTarget.value as TaskType | '')}>
            <option value="">{t('taskType.unset')}</option>
            {#each TASK_TYPES as ty (ty)}<option value={ty}>{t(`taskType.${ty}` as Key)}</option>{/each}
          </select>
        </label>
        <div class="field">
          <span>{t('profile.label')}</span>
          <ProfileSelect value={profile} cwd={project} {taskType} {agent} onchange={setProfile} />
        </div>
      </div>
    {:else if app.profiles}
      <div class="field">
        <span>{t('profile.label')}</span>
        <ProfileSelect value={profile} cwd={project} {agent} onchange={setProfile} />
      </div>
    {/if}
    {#if effortOptions.length && effort}
      <div class="field">
        <span>{t('task.effort')}</span>
        <Segmented
          label={t('task.effort')}
          value={effort}
          options={effortOptions.map((v) => ({ value: v, label: t(`effort.${v}` as Key) }))}
          onchange={(v) => (effort = v)}
        />
      </div>
    {/if}
    <QuorumField agents={app.agents} {agent} on={quorumOn} value={quorum} onon={(v) => (quorumOn = v)} onchange={(q) => (quorum = q)} />
    {#if info && !info.turns}
      <p class="hint faint">{t('task.noTurns', { agent: info.name })}</p>
    {/if}
    <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
  </form>
  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn primary" disabled={busy || uploading > 0} onclick={submit}>{t('task.create')}</button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  label,
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  label > span,
  .field > span {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  textarea {
    resize: vertical;
    min-height: 72px;
  }
  .row {
    display: flex;
    gap: 12px;
  }
  :global(.phone) .row {
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .link {
    color: var(--accent);
  }
  .drop {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .drop.on {
    border-color: var(--accent);
    background: var(--bg-subtle);
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .files li {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    padding: 1px 2px 1px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11px;
  }
  .files li .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    margin: -6px 0 0;
    font-size: 12px;
  }
  .err {
    font-style: normal;
    font-size: 12px;
    color: var(--st-failed);
  }
  input[aria-invalid='true'] {
    border-color: var(--st-failed);
  }
</style>
