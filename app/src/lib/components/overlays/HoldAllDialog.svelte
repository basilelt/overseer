<script lang="ts">
  import { onMount } from 'svelte';
  import Modal from '../ui/Modal.svelte';
  import AgentIcon from '../sidebar/AgentIcon.svelte';
  import { app } from '../../state.svelte';
  import { t } from '../../i18n.svelte';
  import type { HoldPlan } from '../../daemon/api';
  import { isOff } from '../../session/grouping';
  import { targetItems, targetName } from '../../workspace/wsactions';

  let { onclose }: { onclose: () => void } = $props();

  const arg = app.overlayArg;
  const mode: 'hold' | 'resume' | 'close' = arg === 'resume' || arg === 'close' ? arg : 'hold';
  const checked = mode !== 'resume';
  const target = app.broadcastTarget ?? { kind: 'items' as const, keys: [] };
  const name = targetName(target);
  const lines = targetItems(target).filter((i) => i.lineId && (mode === 'close' || (mode === 'hold' ? !isOff(i) : isOff(i))));

  let plans = $state<Record<string, HoldPlan | 'loading'>>({});
  let chosen = $state(new Set<string>());
  let busy = $state(false);
  let done = $state<Record<string, string>>({});

  const okOf = (p: HoldPlan | 'loading' | undefined) => !!p && p !== 'loading' && !p.error && Object.values(p.predicates ?? {}).every((c) => c.ok);
  const failures = (p: HoldPlan | 'loading' | undefined) =>
    !p || p === 'loading' ? [] : [...(p.error ? [p.error] : []), ...Object.entries(p.predicates ?? {}).filter(([, c]) => !c.ok).map(([k, c]) => `${k}: ${c.detail ?? ''}`)];

  onMount(async () => {
    if (mode === 'resume') {
      chosen = new Set(lines.map((l) => l.lineId!));
      return;
    }
    plans = Object.fromEntries(lines.map((l) => [l.lineId!, 'loading' as const]));
    await Promise.all(
      lines.map(async (l) => {
        const p = await (mode === 'close' ? app.api.closePlan : app.api.holdPlan)(l.lineId!).catch((e) => ({ error: String((e as Error)?.message ?? e) }) as HoldPlan);
        plans = { ...plans, [l.lineId!]: p };
        if (okOf(p)) chosen = new Set([...chosen, l.lineId!]);
      }),
    );
  });

  function toggle(id: string) {
    const next = new Set(chosen);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    chosen = next;
  }

  async function apply() {
    busy = true;
    for (const id of chosen) {
      try {
        if (mode === 'hold') await app.api.hold(id);
        else if (mode === 'close') await app.api.close(id);
        else await app.resumeLine({ key: `line:${id}`, lineId: id, kind: '', group: 'stopped', state: 'closed', title: '', at: '' });
        done = { ...done, [id]: 'ok' };
      } catch (e) {
        done = { ...done, [id]: String((e as Error)?.message ?? e) };
      }
    }
    busy = false;
    await app.refresh().catch(() => {});
  }

  const finished = $derived(Object.keys(done).length > 0 && !busy);
</script>

<Modal title={t(mode === 'hold' ? 'hold.titleHold' : mode === 'close' ? 'hold.titleClose' : 'hold.titleResume', { name })} {onclose} width={520}>
  <p class="intro muted">{t(mode === 'hold' ? 'hold.introHold' : mode === 'close' ? 'hold.introClose' : 'hold.introResume')}</p>
  <ul class="rows">
    {#each lines as l (l.key)}
      {@const id = l.lineId!}
      {@const p = plans[id]}
      {@const ok = mode === 'resume' || okOf(p)}
      <li>
        <label class:off={!ok}>
          <input type="checkbox" disabled={!ok || finished} checked={chosen.has(id)} onchange={() => toggle(id)} />
          <AgentIcon kind={l.kind} size={12} />
          <span class="title">{l.title}</span>
          {#if done[id]}
            <span class="st" class:bad={done[id] !== 'ok'}>{done[id] === 'ok' ? t(mode === 'hold' ? 'hold.held' : mode === 'close' ? 'hold.closed' : 'hold.resumed') : done[id]}</span>
          {:else if p === 'loading'}
            <span class="st">{t('common.loading')}</span>
          {:else if checked}
            <span class="st" class:good={ok} class:bad={!ok}>{ok ? t(mode === 'close' ? 'hold.safeClose' : 'hold.safe') : failures(p).join(' · ')}</span>
          {/if}
        </label>
      </li>
    {:else}
      <li class="faint none">{t('hold.none')}</li>
    {/each}
  </ul>
  {#snippet footer()}
    {#if finished}
      <button class="btn primary" onclick={onclose}>{t('broadcast.done')}</button>
    {:else}
      <button class="btn ghost" onclick={onclose}>{t('common.cancel')}</button>
      <button class="btn primary" disabled={!chosen.size || busy} onclick={apply}>{t(mode === 'hold' ? 'hold.confirmHold' : mode === 'close' ? 'hold.confirmClose' : 'hold.confirmResume', { n: chosen.size })}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .intro {
    margin: 0 0 10px;
    font-size: 12.5px;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    max-height: 300px;
    overflow: auto;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 4px;
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
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .good {
    color: var(--st-input);
  }
  .bad {
    color: var(--st-failed);
  }
  .none {
    padding: 8px;
    font-size: 12px;
  }
</style>
