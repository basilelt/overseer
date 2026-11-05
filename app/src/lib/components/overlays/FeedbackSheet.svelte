<script lang="ts">
  import { dropZone } from '../../drop';
  import { onMount } from 'svelte';
  import Paperclip from '@lucide/svelte/icons/paperclip';
  import X from '@lucide/svelte/icons/x';
  import Copy from '@lucide/svelte/icons/copy';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { isTauri, DaemonError } from '../../client';
  import { settings } from '../../settings.svelte';
  import { findAgent, pickAgent } from '../../agents';
  import { basename, MAX_UPLOAD } from '../../uploads';
  import { openExternal } from '../../ports.svelte';
  import { diagBackend, fetchServerLog } from '../../diag.svelte';
  import { feedback, ft, type Result } from '../../feedback.svelte';
  import { APP_LINES, buildBody, buildDiagnostics, buildPrompt, DEFAULT_REPO, findClone, issueTitle, labelsFor, newIssueUrl, SERVER_LINES, validate, wantsDiagnostics, type Kind } from '../../feedback';
  import type { Term } from '../../types';

  const prefill = feedback.prefill;
  const repo = $derived((settings.feedbackRepo ?? '').trim() || DEFAULT_REPO);
  const agentId = $derived(pickAgent(app.agents, settings.defaultAgent));
  const agent = $derived(findAgent(app.agents, agentId));
  const canAgent = $derived(app.connected && !!agent?.installed && agent.id !== 'shell');
  const host = $derived(app.conn.target.kind === 'local' ? 'local' : app.conn.target.label || app.conn.target.kind);

  let kind = $state<Kind>(prefill?.kind ?? 'bug');
  let title = $state(prefill?.title ?? '');
  let description = $state(prefill?.description ?? '');
  let attachments = $state<{ name: string; path: string }[]>([]);
  let includeDiag = $state(wantsDiagnostics(prefill?.kind ?? 'bug'));
  let investigate = $state(false);
  let showPreview = $state(false);
  let diagText = $state<string | null>(null);
  let busy = $state(false);
  let uploading = $state(0);
  let dropping = $state(false);
  let zone = $state<HTMLElement | null>(null);
  let picker = $state<HTMLInputElement | null>(null);

  const form = $derived({ kind, title, description, attachments: attachments.map((a) => a.path), diagnostics: includeDiag ? diagText : null });
  const body = $derived(buildBody(form));
  const fullTitle = $derived(issueTitle(form));
  const kinds: Kind[] = ['bug', 'idea', 'other'];
  const placeholder = $derived(ft(kind === 'bug' ? 'descriptionBug' : kind === 'idea' ? 'descriptionIdea' : 'descriptionOther'));

  const msg = (e: unknown) => String((e as Error)?.message ?? e);

  async function loadDiagnostics() {
    const client = app.client;
    const [info, version, appTail, server] = await Promise.all([
      diagBackend.info().catch(() => null),
      client && app.connected ? client.request<{ version?: string; os?: string; arch?: string }>('GET', '/v1/version').catch(() => null) : Promise.resolve(null),
      prefill?.crash ? Promise.resolve({ lines: prefill.crash.appLog }) : diagBackend.tail(APP_LINES).catch(() => ({ lines: [] as string[] })),
      fetchServerLog(client, app.connected, SERVER_LINES),
    ]);
    diagText = buildDiagnostics({
      appVersion: info?.version ?? '',
      os: info?.os ?? (app.mobile ? 'mobile' : ''),
      arch: info?.arch ?? '',
      serverVersion: version?.version ?? app.conn.version ?? '',
      hostOs: version?.os ?? '',
      hostArch: version?.arch ?? '',
      mode: app.connected ? app.conn.target.kind : 'offline',
      crash: prefill?.crash ? [prefill.crash.summary, prefill.crash.report].filter(Boolean).join('\n\n') : null,
      appLog: appTail.lines,
      serverLog: server.status === 'ok' ? server.tail.lines : [],
    });
  }

  function problem(): string | null {
    const p = validate(form, repo);
    if (p === 'description') return ft('needDescription');
    if (p === 'repo') return ft('badRepo');
    if (p === 'tooLong') return ft('tooLong');
    return null;
  }

  async function addFiles(files: File[]) {
    const client = app.client;
    if (!client || !app.connected) return app.notify(ft('offline'), 'error');
    for (const f of files) {
      if (f.size > MAX_UPLOAD) {
        app.notify(ft('failed', { error: `${f.name}: > 50 MB` }), 'error');
        continue;
      }
      const name = f.name || 'screenshot.png';
      uploading++;
      try {
        const local = await client.savePaste(name, new Uint8Array(await f.arrayBuffer()));
        const up = await client.uploadFile(local);
        attachments = [...attachments, { name: up.name, path: up.path }];
      } catch (e) {
        app.notify(ft('failed', { error: `${name}: ${msg(e)}` }), 'error');
      } finally {
        uploading--;
      }
    }
  }

  async function addPaths(paths: string[]) {
    const client = app.client;
    if (!client || !app.connected) return app.notify(ft('offline'), 'error');
    for (const p of paths) {
      uploading++;
      try {
        const up = await client.uploadFile(p);
        attachments = [...attachments, { name: up.name, path: up.path }];
      } catch (e) {
        app.notify(ft('failed', { error: `${basename(p)}: ${msg(e)}` }), 'error');
      } finally {
        uploading--;
      }
    }
  }

  function onPaste(e: ClipboardEvent) {
    const files = Array.from(e.clipboardData?.files ?? []);
    if (!files.length) return;
    e.preventDefault();
    void addFiles(files);
  }

  function onDomDrop(e: DragEvent) {
    const files = Array.from(e.dataTransfer?.files ?? []);
    dropping = false;
    if (!files.length) return;
    e.preventDefault();
    void addFiles(files);
  }

  function found(r: Result) {
    feedback.announce(r, (text, kind, action) => app.notify(text, kind, action));
  }

  async function sendWithAgent() {
    const p = problem();
    if (p) return app.notify(p, 'error');
    const client = app.client;
    if (!client || !agent) return;
    busy = true;
    try {
      const cwd = findClone(app.projects, repo);
      const same = agentId === settings.defaultAgent;
      const res = await client.request<{ term: Term }>('POST', '/v1/feedback/start', {
        agent: agentId,
        model: same ? settings.defaultModel || undefined : undefined,
        effort: same && agent.efforts?.includes(settings.defaultEffort) ? settings.defaultEffort : undefined,
        cwd,
        cols: 120,
        rows: 32,
        prompt: buildPrompt({ repo, kind, title: fullTitle, attachments: form.attachments, investigate, inClone: !!cwd }),
        body,
      });
      const term = res.term;
      if (!app.data.terms.some((t) => t.id === term.id)) app.data = { ...app.data, terms: [...app.data.terms, term] };
      app.selectedKey = `term:${term.id}`;
      app.view = 'agents';
      void feedback.watch(client, term.id, found);
      feedback.close();
      app.notify(ft('started', { agent: agent.name }));
    } catch (e) {
      app.notify(ft('failed', { error: unsupported(e) ?? msg(e) }), 'error');
    } finally {
      busy = false;
    }
  }

  async function sendDirect() {
    const p = problem();
    if (p) return app.notify(p, 'error');
    const client = app.client;
    if (!client) return;
    busy = true;
    try {
      const res = await client.request<{ url: string }>('POST', '/v1/feedback/direct', { repo, title: fullTitle, body, labels: labelsFor(kind) });
      feedback.showResult({ url: res.url, comment: false });
    } catch (e) {
      app.notify(ft('failed', { error: unsupported(e) ?? msg(e) }), 'error');
    } finally {
      busy = false;
    }
  }

  function unsupported(e: unknown): string | null {
    return e instanceof DaemonError && (e.status === 404 || e.status === 405) ? ft('tooOld') : null;
  }

  async function openGithub() {
    const p = validate(form, repo);
    if (p === 'description' || p === 'repo') return app.notify(problem() ?? '', 'error');
    try {
      await app.client?.copyText(body);
      app.notify(ft('copiedForGithub'));
    } catch {}
    await openExternal(newIssueUrl(repo, fullTitle, body)).catch((e) => app.notify(msg(e), 'error'));
  }

  async function copyLink(url: string) {
    try {
      await app.client?.copyText(url);
      app.notify(ft('linkCopied'));
    } catch (e) {
      app.notify(msg(e), 'error');
    }
  }

  $effect(() => {
    if (includeDiag && diagText === null && !feedback.result) void loadDiagnostics();
  });

  onMount(() => {
    if (feedback.result) return;
    return dropZone({ el: () => zone?.closest('.modal'), rank: 3, hover: (on) => (dropping = on), drop: (paths) => void addPaths(paths) });
  });

  const close = () => feedback.close();
</script>

{#if feedback.result}
  {@const r = feedback.result}
  <Modal title={ft('resultTitle')} onclose={close}>
    <div class="done">
      <CircleCheck size={20} />
      <div>
        <p class="strong">{ft(r.comment ? 'commented' : 'created')}</p>
        <a class="mono link" href={r.url} onclick={(e) => (e.preventDefault(), void openExternal(r.url))}>{r.url}</a>
        <p class="faint small">{ft('resultHint')}</p>
      </div>
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => copyLink(r.url)}><Copy size={14} />{ft('copyLink')}</button>
      <button class="btn" onclick={() => void openExternal(r.url)}><ExternalLink size={14} />{ft('openLink')}</button>
      <span class="spacer"></span>
      <button class="btn primary" onclick={close}>{ft('done')}</button>
    {/snippet}
  </Modal>
{:else}
  <Modal title={ft('title')} onclose={close} width={600}>
    <div class="sheet" bind:this={zone} onpaste={onPaste} ondragover={(e) => (e.preventDefault(), (dropping = true))} ondragleave={() => (dropping = false)} ondrop={onDomDrop} role="presentation">
      <p class="faint small intro">{ft('intro', { repo })}</p>

      <div class="seg" role="radiogroup" aria-label={ft('kind')}>
        {#each kinds as k (k)}
          <button role="radio" aria-checked={kind === k} class:on={kind === k} onclick={() => ((kind = k), (includeDiag = wantsDiagnostics(k)))}>{ft(k)}</button>
        {/each}
      </div>

      <label class="field">
        <span>{ft('titleLabel')}</span>
        <input bind:value={title} placeholder={ft('titleHint')} maxlength="120" />
      </label>

      <label class="field">
        <span>{ft('description')}</span>
        <textarea bind:value={description} rows="6" {placeholder} spellcheck="true"></textarea>
      </label>

      <div class="field">
        <span>{ft('attachments')}</span>
        <div class="drop" class:on={dropping}>
          <Paperclip size={14} />
          <span class="faint">{ft('attachHint')}</span>
          <button class="linkish" onclick={() => picker?.click()}>{ft('browse')}</button>
          {#if uploading}<span class="faint">· {uploading}…</span>{/if}
          <input bind:this={picker} type="file" multiple hidden onchange={(e) => (void addFiles(Array.from(e.currentTarget.files ?? [])), (e.currentTarget.value = ''))} />
        </div>
        {#if attachments.length}
          <ul class="files">
            {#each attachments as a, i (a.path)}
              <li>
                <span class="mono">{a.name}</span>
                <button class="icon-btn" aria-label={ft('remove', { name: a.name })} title={ft('remove', { name: a.name })} onclick={() => (attachments = attachments.filter((_, j) => j !== i))}><X size={12} /></button>
              </li>
            {/each}
          </ul>
          <p class="faint small">{ft('attachNote')}</p>
        {/if}
      </div>

      <label class="check">
        <input type="checkbox" class="switch" bind:checked={includeDiag} />
        <span>{ft('diagnostics')}<small>{ft('diagnosticsHint', { app: APP_LINES, server: SERVER_LINES })}</small></span>
      </label>
      <label class="check">
        <input type="checkbox" class="switch" bind:checked={investigate} disabled={!canAgent} />
        <span>{ft('investigate')}<small>{ft('investigateHint')}</small></span>
      </label>

      <button class="linkish preview-toggle" onclick={() => (showPreview = !showPreview)}>{showPreview ? ft('hidePreview') : ft('preview')}</button>
      {#if showPreview}
        <pre class="preview"><strong>{fullTitle}</strong>

{includeDiag && diagText === null ? ft('loading') : body}</pre>
      {/if}

      <p class="faint small">
        {#if !app.connected}{ft('offline')}{:else if canAgent && agent}{ft('via', { agent: agent.name, host })}{:else}{ft('noAgent')}{/if}
      </p>
    </div>

    {#snippet footer()}
      <button class="btn" onclick={openGithub} disabled={busy}><ExternalLink size={14} />{ft('github')}</button>
      {#if app.connected}
        <button class="btn" onclick={sendDirect} disabled={busy || uploading > 0} title={ft('directHint')}>{ft('direct')}</button>
      {/if}
      <span class="spacer"></span>
      <button class="btn" onclick={close}>{ft('cancel')}</button>
      {#if canAgent && agent}
        <button class="btn primary" onclick={sendWithAgent} disabled={busy || uploading > 0 || (includeDiag && diagText === null)}>{busy ? ft('sending') : ft('send', { agent: agent.name })}</button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .sheet {
    display: grid;
    gap: 12px;
  }
  .intro {
    margin: 0;
  }
  .small {
    font-size: 11px;
    margin: 0;
  }
  .field {
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .field > span {
    color: var(--text-2);
  }
  textarea {
    resize: vertical;
    min-height: 96px;
    font: inherit;
  }
  .seg {
    justify-self: start;
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .seg button {
    padding: 3px 12px;
    border-radius: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .drop {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .drop.on {
    border-color: var(--accent, var(--text-2));
    background: var(--bg-subtle);
  }
  .linkish {
    padding: 0;
    color: var(--text);
    text-decoration: underline;
    font-size: 12px;
  }
  .preview-toggle {
    justify-self: start;
    color: var(--text-2);
  }
  .files {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .files li {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 2px 1px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11px;
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 12px;
  }
  .check span {
    display: grid;
  }
  .check small {
    color: var(--text-3);
    font-size: 11px;
  }
  .preview {
    margin: 0;
    max-height: 240px;
    overflow: auto;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    font-family: var(--mono);
    font-size: 11px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  .done {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 4px 0;
    color: var(--text);
  }
  .done :global(svg) {
    color: var(--st-input);
    flex: none;
    margin-top: 2px;
  }
  .done p {
    margin: 0 0 4px;
  }
  .strong {
    font-weight: 600;
  }
  .link {
    display: block;
    font-size: 12px;
    overflow-wrap: anywhere;
    margin-bottom: 6px;
  }
  .mono {
    font-family: var(--mono);
  }
  .spacer {
    flex: 1;
  }
</style>
