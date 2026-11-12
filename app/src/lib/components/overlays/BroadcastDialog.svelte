<script lang="ts">
  import { onMount } from 'svelte';
  import Modal from '../ui/Modal.svelte';
  import AgentIcon from '../sidebar/AgentIcon.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import { broadcastBody, summarize, targetRows, type TargetState } from '../../broadcast';
  import { targetItems, targetName } from '../../wsactions';
  import type { BroadcastResult } from '../../types';
  import { isSubmitChord } from '../../chord';

  let { onclose }: { onclose: () => void } = $props();

  const target = app.broadcastTarget ?? { kind: 'items' as const, keys: [] };
  const name = targetName(target);
  let text = $state('');
  let queue = $state(false);
  let picked = $state<Set<string> | null>(null);
  let busy = $state(false);
  let error = $state('');
  let results = $state<BroadcastResult[] | null>(null);
  let delivered = $state<Record<string, string>>({});

  const items = $derived(targetItems(target));
  const rows = $derived(targetRows(items, queue));
  const chosen = $derived(picked ?? new Set(rows.filter((r) => r.checked).map((r) => r.key)));
  const selected = $derived(rows.filter((r) => r.selectable && chosen.has(r.key)));
  const ws = $derived(target.kind === 'workspace' ? app.ws.workspaces.find((w) => w.id === target.id) : undefined);
  const group = $derived(target.kind === 'group' ? app.ws.groups.find((g) => g.id === target.id) : undefined);
  const notifyIdle = $derived(ws?.notifyIdle ?? group?.notifyIdle ?? false);
  const titleOf = (key: string) => rows.find((r) => r.key === key)?.title ?? key;

  onMount(() =>
    app.onBroadcast((d) => {
      if (results?.some((r) => r.target === d.target && r.result === 'queued')) delivered = { ...delivered, [d.target]: d.result };
    }),
  );

  function toggle(key: string) {
    const next = new Set(chosen);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    picked = next;
  }

  function setQueue(v: boolean) {
    queue = v;
    picked = null;
  }

  async function toggleNotify() {
    if (ws) await app.patchWorkspace(ws.id, { notifyIdle: !ws.notifyIdle });
    else if (group) {
      const next = await app.api.patchGroup(group.id, { notifyIdle: !group.notifyIdle }).catch(() => null);
      if (next) app.ws = { ...app.ws, groups: app.ws.groups.map((g) => (g.id === next.id ? next : g)) };
    }
  }

  async function send() {
    if (!text.trim() || !selected.length || busy) return;
    busy = true;
    error = '';
    try {
      const r = await app.api.broadcast(broadcastBody(selected.map((s) => s.key), items, text, queue));
      results = r.results;
    } catch (e) {
      error = String((e as Error)?.message ?? e);
    } finally {
      busy = false;
    }
  }

  const stateLabel: Record<TargetState, () => string> = {
    idle: () => t('broadcast.idle'),
    busy: () => t('broadcast.busy'),
    needs_input: () => t('wsstate.needs_input'),
    paused: () => t('group.paused'),
    stopped: () => t('group.stopped'),
  };
  const sum = $derived(results ? summarize(results) : null);

  function autofocus(node: HTMLElement) {
    node.focus();
  }
</script>

<Modal title={t('broadcast.to', { name })} {onclose} width={520}>
  {#if !results}
    <form
      class="form"
      onsubmit={(e) => {
        e.preventDefault();
        send();
      }}
    >
      <textarea
        use:autofocus
        rows="4"
        bind:value={text}
        placeholder={t('broadcast.placeholder')}
        aria-label={t('broadcast.message')}
        spellcheck="true"
        onkeydown={(e) => {
          if (isSubmitChord(e)) {
            e.preventDefault();
            send();
          }
        }}
      ></textarea>
      <div class="head">
        <span>{t('broadcast.recipients', { n: selected.length, total: rows.length })}</span>
        <label class="opt"><input type="checkbox" checked={queue} onchange={(e) => setQueue(e.currentTarget.checked)} />{t('broadcast.queue')}</label>
      </div>
      <ul class="rows" role="group" aria-label={t('broadcast.recipientsLabel')}>
        {#each rows as r (r.key)}
          <li>
            <label class:off={!r.selectable}>
              <input type="checkbox" disabled={!r.selectable} checked={r.selectable && chosen.has(r.key)} onchange={() => toggle(r.key)} />
              <AgentIcon kind={r.kind} size={12} />
              <span class="title">{r.title}</span>
              <span class="st {r.state}">{stateLabel[r.state]()}</span>
            </label>
          </li>
        {:else}
          <li class="faint none">{t('broadcast.noTargets')}</li>
        {/each}
      </ul>
      {#if ws || group}
        <label class="opt"><input type="checkbox" checked={notifyIdle} onchange={toggleNotify} />{t('broadcast.notifyIdle')}</label>
      {/if}
      <small class="faint">{t('broadcast.hint')}</small>
      {#if error}<em class="err" role="alert">{error}</em>{/if}
    </form>
  {:else}
    <p class="sum">
      {t('broadcast.summary', { sent: sum!.sent, queued: sum!.queued, skipped: sum!.skipped + sum!.refused })}
    </p>
    <ul class="rows">
      {#each results as r (r.target)}
        {@const final = delivered[r.target]}
        <li class="res">
          <span class="title">{titleOf(r.target)}</span>
          <span class="st {final ?? r.result}">{final ? t(`broadcast.r.${final === 'sent' ? 'delivered' : 'dropped'}`) : t(`broadcast.r.${r.result}`)}{r.reason && !final ? ` · ${r.reason}` : ''}</span>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    {#if results}
      <button class="btn primary" onclick={onclose}>{t('broadcast.done')}</button>
    {:else}
      <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
      <button class="btn primary" disabled={!text.trim() || !selected.length || busy} onclick={send}>{t('broadcast.send', { n: selected.length })}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  textarea {
    resize: vertical;
    min-height: 84px;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .opt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
    font-weight: 400;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    max-height: 240px;
    overflow: auto;
  }
  .rows label,
  .res {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 4px;
  }
  .rows label:hover:not(.off) {
    background: var(--surface-hover);
  }
  .off {
    color: var(--text-3);
  }
  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st {
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .st.idle,
  .st.sent {
    color: var(--st-input);
  }
  .st.busy,
  .st.queued {
    color: var(--st-working);
  }
  .st.needs_input {
    color: var(--st-testing);
  }
  .st.refused {
    color: var(--st-failed);
  }
  .none {
    padding: 8px;
    font-size: 12px;
  }
  small {
    font-size: 11.5px;
  }
  .err {
    font-style: normal;
    font-size: 12px;
    color: var(--st-failed);
  }
  .sum {
    margin: 0 0 10px;
  }
</style>
