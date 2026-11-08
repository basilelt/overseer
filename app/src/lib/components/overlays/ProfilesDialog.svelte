<script lang="ts">
  import { untrack } from 'svelte';
  import Plus from '@lucide/svelte/icons/plus';
  import Copy from '@lucide/svelte/icons/copy';
  import Trash from '@lucide/svelte/icons/trash-2';
  import Modal from '../ui/Modal.svelte';
  import ModelField from '../ui/ModelField.svelte';
  import AgentGlyph from '../ui/AgentGlyph.svelte';
  import { findAgent, taskAgents } from '../../agents';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { settings } from '../../settings.svelte';
  import { chipLabel, clean, costIndex, formatIndex, NONE, PRICES_AS_OF, slugId, validateProfile } from '../../profiles';
  import type { AgentInfo, Effort, FileEdit, Profile, ProfileAgent, ProfileSupport, SubagentDef } from '../../types';

  let { onclose, scope = null }: { onclose: () => void; scope?: string | null } = $props();

  const store = $derived(app.profiles);
  const wsPath = $derived(scope ?? app.currentWorkspace?.path ?? null);
  const wsName = $derived(app.ws.workspaces.find((w) => !!wsPath && w.path === wsPath)?.name ?? wsPath ?? '');
  const agents = $derived(taskAgents(app.agents).filter((a) => a.installed && store?.support[a.id] && Object.keys(store.support[a.id]).length));

  let draft = $state<Profile[]>(structuredClone($state.snapshot(app.profiles?.profiles.filter((p) => !p.builtin) ?? [])));
  let selected = $state(untrack(() => app.profiles?.scopes[scope ?? ''] || app.profiles?.default || 'balanced'));
  let busy = $state(false);
  let subs = $state<SubagentDef[] | null>(null);
  let preview = $state<FileEdit[] | null>(null);
  let chosen = $state<string[]>([]);
  let applied = $state<FileEdit[] | null>(null);
  let expanded = $state<Record<string, boolean>>({});

  const all = $derived([...(store?.profiles.filter((p) => p.builtin) ?? []), ...draft]);
  const current = $derived(all.find((p) => p.id === selected) ?? all[0]);
  const editable = $derived(!!current && !current.builtin);
  const saved = $derived(JSON.stringify(store?.profiles.filter((p) => !p.builtin) ?? []));
  const dirty = $derived(JSON.stringify(draft.map((p) => ({ ...p, agents: Object.fromEntries(Object.entries(p.agents).map(([k, v]) => [k, clean(v)]).filter(([, v]) => Object.keys(v).length)) }))) !== saved);
  const errors = $derived(
    current && editable && store
      ? validateProfile(
          current,
          store.support,
          app.agents,
          draft.filter((p) => p !== current),
        )
      : [],
  );
  const allErrors = $derived(store ? draft.flatMap((p) => validateProfile(p, store.support, app.agents, draft.filter((o) => o !== p))) : []);

  const effortName = (e: string) => t(`effort.${e}` as Key);
  const via = (s: ProfileSupport, k: keyof ProfileSupport) => (s[k] ? t(`profile.via.${s[k]}` as Key) : '');

  function cfg(agent: string): ProfileAgent {
    return current?.agents[agent] ?? {};
  }

  function set(agent: string, patch: Partial<ProfileAgent>) {
    if (!editable || !current) return;
    current.agents[agent] = { ...cfg(agent), ...patch };
  }

  function setSub(agent: string, name: string, patch: { model?: string; effort?: Effort | '' }) {
    const c = cfg(agent);
    const next = { ...(c.subagents?.[name] ?? {}), ...patch } as { model?: string; effort?: Effort };
    set(agent, { subagents: { ...(c.subagents ?? {}), [name]: next } });
  }

  function summary(a: AgentInfo): string {
    const c = cfg(a.id);
    const idx = formatIndex(costIndex(current, a.id, store?.support[a.id]));
    return [chipLabel({ model: c.model, effort: c.effort }, (e) => t('profile.effortOf', { effort: effortName(e).toLowerCase() })) || t('profile.cliDefaults'), idx].filter(Boolean).join(' · ');
  }

  function add(from?: Profile) {
    const name = from ? t('profile.copyOf', { name: from.name }) : t('profile.newName');
    const p: Profile = { id: slugId(name, all.map((x) => x.id)), name, agents: structuredClone($state.snapshot(from?.agents ?? {})) };
    draft.push(p);
    selected = p.id;
  }

  function remove() {
    if (!editable) return;
    draft = draft.filter((p) => p.id !== selected);
    selected = store?.default || 'balanced';
  }

  async function save() {
    if (!store || allErrors.length) return;
    busy = true;
    try {
      const profiles = draft.map((p) => ({ id: p.id, name: p.name.trim(), agents: Object.fromEntries(Object.entries(p.agents).map(([k, v]) => [k, clean(v)])) }));
      const ids = new Set([...store.profiles.filter((p) => p.builtin).map((p) => p.id), ...profiles.map((p) => p.id)]);
      const known = (id: string) => !id || id === NONE || ids.has(id);
      await app.saveProfiles({
        profiles,
        default: known(store.default) ? store.default : '',
        scopes: Object.fromEntries(Object.entries(store.scopes).filter(([, id]) => known(id))),
      });
      app.notify(t('profile.saved'));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      busy = false;
    }
  }

  async function setDefault(id: string) {
    await app.saveProfiles({ default: id }).catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
  }

  async function setScope(id: string) {
    if (!store || !wsPath) return;
    const scopes = { ...store.scopes };
    if (id) scopes[wsPath] = id;
    else delete scopes[wsPath];
    await app.saveProfiles({ scopes }).catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
  }

  async function loadSubs() {
    subs = (await app.api.subagents(wsPath ?? undefined).catch(() => ({ subagents: [] }))).subagents;
  }

  async function previewFiles() {
    if (!current) return;
    applied = null;
    try {
      preview = (await app.api.applyProfileFiles({ profile: current.id, cwd: wsPath ?? undefined, dryRun: true })).files;
      chosen = [];
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async function applyFiles() {
    if (!current || !chosen.length) return;
    busy = true;
    try {
      applied = (await app.api.applyProfileFiles({ profile: current.id, cwd: wsPath ?? undefined, dryRun: false, files: chosen })).files;
      preview = null;
      await loadSubs();
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      busy = false;
    }
  }

  function toggle(path: string, on: boolean) {
    chosen = on ? [...chosen, path] : chosen.filter((p) => p !== path);
  }
</script>

<Modal title={t('profile.title')} {onclose} width={760}>
  {#if !store}
    <p class="faint">{t('profile.unsupported')}</p>
  {:else}
    <section class="scopes">
      <label class="row">
        <span>{t('profile.globalDefault')}<small>{t('profile.globalHint')}</small></span>
        <select value={store.default || NONE} onchange={(e) => setDefault(e.currentTarget.value === NONE ? '' : e.currentTarget.value)}>
          <option value={NONE}>{t('profile.none')}</option>
          {#each store.profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      </label>
      {#if wsPath}
        <label class="row">
          <span>{t('profile.workspace', { name: wsName })}<small class="mono">{wsPath}</small></span>
          <select value={store.scopes[wsPath] ?? ''} onchange={(e) => setScope(e.currentTarget.value)}>
            <option value="">{t('profile.inheritGlobal')}</option>
            {#each store.profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            <option value={NONE}>{t('profile.none')}</option>
          </select>
        </label>
      {/if}
    </section>

    <div class="body">
      <nav aria-label={t('profile.list')}>
        {#each all as p (p.id)}
          <button class:on={p.id === current?.id} onclick={() => (selected = p.id)}>
            <span class="pname">{p.name}</span>
            <span class="faint mono">{formatIndex(costIndex(p, settings.defaultAgent, store.support[settings.defaultAgent]))}</span>
          </button>
        {/each}
        <button class="add" onclick={() => add()}><Plus size={13} />{t('profile.new')}</button>
      </nav>

      {#if current}
        <div class="editor">
          <div class="head">
            {#if editable}
              <input class="name" aria-label={t('profile.name')} bind:value={current.name} maxlength="40" />
            {:else}
              <h3>{current.name}<span class="badge">{t('profile.builtin')}</span></h3>
            {/if}
            <span class="spacer"></span>
            <button class="btn ghost" onclick={() => add(current)}><Copy size={13} />{t('profile.duplicate')}</button>
            {#if editable}<button class="btn ghost danger" onclick={remove}><Trash size={13} />{t('common.delete')}</button>{/if}
          </div>
          {#if !editable}<p class="faint hint">{t('profile.builtinHint')}</p>{/if}

          {#each agents as a (a.id)}
            {@const s = store.support[a.id]}
            {@const c = cfg(a.id)}
            <details class="agent" open={expanded[a.id] ?? agents.length === 1} ontoggle={(e) => (expanded[a.id] = e.currentTarget.open)}>
              <summary>
                <AgentGlyph agent={a.id} size={13} />
                <span class="aname">{a.name}</span>
                <span class="faint mono sum">{summary(a)}</span>
              </summary>
              <fieldset disabled={!editable}>
                {#if s.model}
                  <div class="row">
                    <span>{t('profile.model')}<small>{via(s, 'model')}</small></span>
                    <div class="ctl"><ModelField agent={a} value={c.model ?? ''} label={t('profile.model')} onchange={(v) => set(a.id, { model: v })} /></div>
                  </div>
                {/if}
                {#if s.effort && a.efforts.length}
                  <div class="row">
                    <span>{t('profile.effort')}<small>{via(s, 'effort')}</small></span>
                    <select class="ctl" value={c.effort ?? ''} onchange={(e) => set(a.id, { effort: (e.currentTarget.value || undefined) as Effort })}>
                      <option value="">{t('model.default')}</option>
                      {#each a.efforts as e}<option value={e}>{effortName(e)}</option>{/each}
                    </select>
                  </div>
                {/if}
                {#if s.subagentModel}
                  <div class="row">
                    <span>{t('profile.subagentModel')}<small>{via(s, 'subagentModel')}</small></span>
                    <div class="ctl"><ModelField agent={a} value={c.subagentModel ?? ''} label={t('profile.subagentModel')} onchange={(v) => set(a.id, { subagentModel: v })} /></div>
                  </div>
                  {#if s.subagentForce}
                    <label class="row">
                      <span>{t('profile.subagentForce')}<small>{t('profile.subagentForceHint')}</small></span>
                      <input type="checkbox" checked={!!c.subagentForce} disabled={!c.subagentModel} onchange={(e) => set(a.id, { subagentForce: e.currentTarget.checked })} />
                    </label>
                  {/if}
                {/if}
                {#if s.subagentEffort && a.efforts.length}
                  <div class="row">
                    <span>{t('profile.subagentEffort')}<small>{via(s, 'subagentEffort')}</small></span>
                    <select class="ctl" value={c.subagentEffort ?? ''} onchange={(e) => set(a.id, { subagentEffort: (e.currentTarget.value || undefined) as Effort })}>
                      <option value="">{t('profile.sameAsMain')}</option>
                      {#each a.efforts as e}<option value={e}>{effortName(e)}</option>{/each}
                    </select>
                  </div>
                {/if}
                {#if s.smallModel}
                  <div class="row">
                    <span>{t(a.id === 'claude' ? 'profile.smallModelClaude' : 'profile.smallModel')}<small>{via(s, 'smallModel')}</small></span>
                    <input class="ctl mono" value={c.smallModel ?? ''} placeholder={t('model.default')} spellcheck="false" oninput={(e) => set(a.id, { smallModel: e.currentTarget.value.trim() })} />
                  </div>
                {/if}
              </fieldset>
              {#if s.subagents}
                <div class="subs">
                  <div class="row">
                    <span>{t('profile.namedSubagents')}<small>{t(s.subagents === 'file' ? 'profile.namedFileHint' : 'profile.namedEnvHint')}</small></span>
                    {#if !subs}<button class="btn" onclick={loadSubs}>{t('profile.loadSubagents')}</button>{/if}
                  </div>
                  {#if subs}
                    {@const mine = subs.filter((d) => d.agent === a.id)}
                    {#if !mine.length}<p class="faint hint">{t('profile.noSubagents')}</p>{/if}
                    {#each mine as d (d.path + d.name)}
                      {@const o = c.subagents?.[d.name] ?? {}}
                      <div class="sub">
                        <span class="sname" title={d.path}>{d.name}<small class="faint">{t(`profile.scope.${d.scope}` as Key)} · {d.model || t('profile.inherits')}{d.effort ? ` · ${effortName(d.effort)}` : ''}</small></span>
                        <div class="sctl">
                          {#if d.agent === 'agy'}
                            <select value={o.model ?? 'inherit'} disabled={!editable} aria-label={d.name} onchange={(e) => setSub(a.id, d.name, { model: e.currentTarget.value })}>
                              <option value="inherit">{t('profile.inherits')}</option>
                              <option value="flash">{t('profile.subagentTier.flash')}</option>
                              <option value="pro">{t('profile.subagentTier.pro')}</option>
                            </select>
                          {:else}
                            <ModelField agent={a} value={o.model ?? ''} label={d.name} onchange={(v) => editable && setSub(a.id, d.name, { model: v })} />
                          {/if}
                          {#if s.subagentEfforts && a.efforts.length}
                            <select value={o.effort ?? ''} disabled={!editable} aria-label={t('profile.effort')} onchange={(e) => setSub(a.id, d.name, { effort: e.currentTarget.value as Effort | '' })}>
                              <option value="">{t('profile.keep')}</option>
                              {#each a.efforts as e}<option value={e}>{effortName(e)}</option>{/each}
                            </select>
                          {/if}
                        </div>
                      </div>
                    {/each}
                    {#if s.subagents === 'file' && mine.length}
                      <div class="apply">
                        <button class="btn" disabled={dirty || busy} title={dirty ? t('profile.saveFirst') : ''} onclick={previewFiles}>{t('profile.previewFiles')}</button>
                        {#if dirty}<small class="faint">{t('profile.saveFirst')}</small>{/if}
                      </div>
                    {/if}
                  {/if}
                </div>
              {/if}
            </details>
          {/each}

          {#if preview}
            <div class="preview" role="region" aria-label={t('profile.previewFiles')}>
              {#if !preview.length}
                <p class="faint">{t('profile.nothingToApply')}</p>
              {:else}
                <p class="hint">{t('profile.confirmHint')}</p>
                {#each preview as f (f.path)}
                  <label class="file">
                    <input type="checkbox" checked={chosen.includes(f.path)} onchange={(e) => toggle(f.path, e.currentTarget.checked)} />
                    <span>
                      <span class="mono">{f.path}</span>
                      {#each f.changes as ch}
                        {#if ch.from}<code class="del">- {ch.key}: {ch.from}</code>{/if}
                        <code class="ins">+ {ch.key}: {ch.to}</code>
                      {/each}
                    </span>
                  </label>
                {/each}
                <div class="apply">
                  <button class="btn ghost" onclick={() => (preview = null)}>{t('common.cancel')}</button>
                  <button class="btn primary" disabled={!chosen.length || busy} onclick={applyFiles}>{t('profile.applyN', { n: chosen.length })}</button>
                </div>
              {/if}
            </div>
          {/if}
          {#if applied}
            <div class="preview">
              {#each applied as f (f.path)}
                <p class="mono small">{f.written ? '✓' : '✗'} {f.path}{f.backup ? ` → ${t('profile.backup')} ${f.backup}` : ''}{f.error ? ` · ${f.error}` : ''}</p>
              {/each}
            </div>
          {/if}

          {#if errors.length}<ul class="errs">{#each errors as e}<li>{e}</li>{/each}</ul>{/if}
          <p class="faint note">{t('profile.costNote', { date: PRICES_AS_OF })}</p>
        </div>
      {/if}
    </div>
  {/if}
  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>{t('common.close')}</button>
    <button class="btn primary" disabled={!dirty || busy || allErrors.length > 0} onclick={save}>{t('profile.save')}</button>
  {/snippet}
</Modal>

<style>
  .scopes {
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 34px;
  }
  .row > span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .row > span > small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .scopes select {
    flex: none;
    max-width: 50%;
  }
  small {
    color: var(--text-3);
    font-size: 11px;
  }
  .ctl {
    width: 240px;
    flex: none;
  }
  .body {
    display: flex;
    gap: 16px;
    padding-top: 12px;
    min-height: 320px;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 168px;
    flex: none;
  }
  nav button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 6px 8px;
    border-radius: var(--radius);
    color: var(--text-2);
    text-align: left;
  }
  nav button.on {
    background: var(--accent-soft);
    color: var(--text);
  }
  nav button:hover:not(.on) {
    background: var(--surface-hover);
  }
  .pname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  nav .add {
    justify-content: flex-start;
    color: var(--text-3);
    margin-top: 4px;
  }
  .editor {
    flex: 1;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }
  .head h3 {
    margin: 0;
    font-size: 14px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    font-size: 14px;
    font-weight: 600;
    width: 220px;
  }
  .spacer {
    flex: 1;
  }
  .badge {
    font-size: 10px;
    font-weight: 500;
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--surface-2);
    color: var(--text-3);
  }
  .hint {
    margin: 0 0 8px;
    font-size: 11.5px;
  }
  .agent {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 8px;
  }
  summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .aname {
    font-weight: 500;
    white-space: nowrap;
  }
  .sum {
    margin-left: auto;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  fieldset {
    border: 0;
    margin: 0;
    padding: 0 10px 6px;
  }
  .subs {
    padding: 4px 10px 10px;
    border-top: 1px solid var(--border);
  }
  .sub {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
  }
  .sname {
    display: flex;
    flex-direction: column;
    min-width: 0;
    font-size: 12.5px;
  }
  .sctl {
    display: flex;
    gap: 6px;
    width: 300px;
    flex: none;
  }
  .sctl > :global(.model) {
    flex: 1;
  }
  .apply {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .preview {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    margin: 8px 0;
    background: var(--bg-subtle);
  }
  .file {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 4px 0;
  }
  .file > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    font-size: 11.5px;
    word-break: break-all;
  }
  code {
    font-family: var(--mono);
    font-size: 11px;
  }
  .del {
    color: var(--st-failed);
  }
  .ins {
    color: var(--st-input);
  }
  .small {
    margin: 2px 0;
    font-size: 11px;
    word-break: break-all;
  }
  .errs {
    margin: 8px 0;
    padding-left: 18px;
    color: var(--st-failed);
    font-size: 12px;
  }
  .note {
    margin: 10px 0 0;
    font-size: 11px;
  }
  :global(.phone) .body {
    flex-direction: column;
  }
  :global(.phone) nav {
    width: auto;
  }
  :global(.phone) .ctl,
  :global(.phone) .sctl {
    width: 160px;
  }
  :global(.phone) .head {
    flex-wrap: wrap;
  }
  :global(.phone) .head .name {
    width: 100%;
    min-width: 0;
  }
  :global(.phone) .head .spacer {
    display: none;
  }
</style>
