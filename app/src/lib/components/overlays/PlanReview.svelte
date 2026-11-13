<script lang="ts">
  import { untrack } from 'svelte';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleMinus from '@lucide/svelte/icons/circle-minus';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Modal from '../ui/Modal.svelte';
  import { app } from '../../state.svelte';
  import { t, type Key } from '../../i18n.svelte';
  import { organizer } from '../../workspace/organize.svelte';
  import { checkedIds, describe, grouped, initialChecks, tally, type Action, type Proposal, type Result } from '../../workspace/organize';

  let { proposal }: { proposal: Proposal } = $props();

  let checked = $state<Record<string, boolean>>(untrack(() => initialChecks(proposal.actions)));
  const sections = $derived(grouped(proposal.actions));
  const ids = $derived(checkedIds(proposal.actions, checked));
  const results = $derived(organizer.results);
  const byId = $derived(new Map((results ?? []).map((r) => [r.actionId, r])));
  const counts = $derived(results ? tally(results) : null);
  const names = $derived({ folders: app.ws.folders, workspaces: app.ws.workspaces, lines: app.data.lines, tasks: app.data.tasks });

  function text(a: Action) {
    const d = describe(a, proposal.actions, names);
    return t(d.key as Key, d.params);
  }

  function setAll(on: boolean, list: Action[]) {
    checked = { ...checked, ...Object.fromEntries(list.map((a) => [a.id, on])) };
  }

  const icon = (r: Result) => (r.status === 'done' ? CircleCheck : r.status === 'skipped' ? CircleMinus : CircleX);
</script>

<Modal title={t('org.title')} onclose={() => organizer.close()} width={640}>
  <div class="plan">
    <p class="summary">
      {#if proposal.summary}<span>{proposal.summary}</span>{/if}
      <span class="faint small">{t(proposal.source === 'agent' ? 'org.source.agent' : 'org.source.deterministic')}</span>
    </p>
    {#if counts}<p class="counts small">{t('org.applied', counts)}</p>{/if}
    {#if !proposal.actions.length}
      <p class="faint">{t('org.empty')}</p>
    {/if}
    {#each sections as s (s.id)}
      <section>
        <header>
          <h4>{t(`org.section.${s.id}` as Key)}</h4>
          {#if !results}
            <button class="linkish" onclick={() => setAll(true, s.actions)}>{t('org.all')}</button>
            <button class="linkish" onclick={() => setAll(false, s.actions)}>{t('org.none')}</button>
          {/if}
        </header>
        <ul>
          {#each s.actions as a (a.id)}
            {@const r = byId.get(a.id)}
            <li class:off={!results && !checked[a.id]}>
              {#if r}
                {@const Icon = icon(r)}
                <span class="res {r.status}" title={t(`org.result.${r.status}` as Key)}><Icon size={15} /></span>
              {:else if results}
                <span class="res"></span>
              {:else}
                <input type="checkbox" bind:checked={checked[a.id]} aria-label={text(a)} />
              {/if}
              <span class="body">
                <span class="what">{text(a)}</span>
                {#if proposal.reasons[a.id]}<span class="why faint">{proposal.reasons[a.id]}</span>{/if}
                {#if a.kind === 'close' && !results}<span class="why faint">{t('org.closeHint')}</span>{/if}
                {#if r?.detail}<span class="why {r.status}">{r.detail}</span>{/if}
              </span>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
  {#snippet footer()}
    {#if !results}
      <button class="btn" onclick={() => void organizer.dismiss()}>{t('org.discard')}</button>
    {/if}
    <span class="spacer"></span>
    {#if proposal.pickUp.length}
      <button class="btn" onclick={() => organizer.pickUp()}>{t('org.pickUpN', { n: proposal.pickUp.length })}</button>
    {/if}
    {#if results}
      <button class="btn primary" onclick={() => organizer.close()}>{t('org.done')}</button>
    {:else}
      <button class="btn primary" disabled={organizer.busy || !ids.length} onclick={() => void organizer.apply(ids)}>{organizer.busy ? t('org.applying') : t('org.apply', { n: ids.length })}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .plan {
    display: grid;
    gap: 14px;
  }
  .summary {
    margin: 0;
    display: grid;
    gap: 2px;
  }
  .small {
    font-size: 11px;
  }
  .counts {
    margin: 0;
    color: var(--text-2);
  }
  section header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 4px;
  }
  h4 {
    margin: 0;
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .linkish {
    padding: 0;
    color: var(--text-2);
    text-decoration: underline;
    font-size: 11px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 8px;
    border-radius: var(--radius);
  }
  li:hover {
    background: var(--bg-subtle);
  }
  li.off .what {
    color: var(--text-3);
  }
  input[type='checkbox'] {
    margin-top: 2px;
    flex: none;
  }
  .res {
    width: 15px;
    flex: none;
    margin-top: 1px;
    color: var(--text-3);
  }
  .res.done {
    color: var(--st-input);
  }
  .res.failed,
  .why.failed {
    color: var(--st-failed);
  }
  .body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .what {
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .why {
    font-size: 12px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
</style>
