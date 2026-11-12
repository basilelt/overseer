<script lang="ts">
  import { untrack } from 'svelte';
  import { tour, hasTarget } from '../../tour/tour.svelte';
  import { keyAction, targetOf, type Step } from '../../tour/steps';
  import { placeCard, union, type Placement, type Rect } from '../../tour/place';
  import { hasKey, tt } from '../../tour/strings';
  import { isPhone } from '../../mobile/ui.svelte';
  import { modLabel } from '../../platform';

  const PAD = 4;
  const phone = $derived(isPhone());

  let card: HTMLElement | undefined = $state();
  let primary: HTMLButtonElement | undefined = $state();
  let rect = $state<Rect | null>(null);
  let pos = $state<Placement | null>(null);
  let target = $state<string | null>(null);
  let below = $state(false);

  function elements(name: string | null): Element[] {
    return name ? [...document.querySelectorAll(`[data-tour="${name}"]`)] : [];
  }

  function measure() {
    const u = union(elements(target).map((e) => e.getBoundingClientRect()));
    rect = u ? { top: u.top - PAD, left: u.left - PAD, width: u.width + PAD * 2, height: u.height + PAD * 2 } : null;
    below = !!rect && rect.top + rect.height / 2 > innerHeight / 2;
    if (!card || phone) return void (pos = null);
    const c = { width: card.offsetWidth, height: card.offsetHeight };
    const v = { width: innerWidth, height: innerHeight };
    pos = rect ? placeCard(rect, c, v) : { top: Math.round((v.height - c.height) / 2), left: Math.round((v.width - c.width) / 2) };
  }

  $effect(() => {
    const step = tour.step;
    if (!step || !card) return;
    const name = untrack(() => targetOf(step, hasTarget));
    const els = elements(name);
    untrack(() => {
      target = name;
      els[0]?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      measure();
    });
    const ro = new ResizeObserver(() => measure());
    for (const el of [...els, card, document.documentElement]) ro.observe(el);
    const opts = { capture: true, passive: true };
    addEventListener('resize', measure);
    addEventListener('scroll', measure, opts);
    return () => {
      ro.disconnect();
      removeEventListener('resize', measure);
      removeEventListener('scroll', measure, opts);
    };
  });

  $effect(() => {
    void tour.phase;
    void tour.index;
    primary?.focus({ preventScroll: true });
  });

  function onKey(e: KeyboardEvent) {
    const a = keyAction(e.key);
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    tour.key(a);
  }

  function onFocus(e: FocusEvent) {
    const dialog = primary?.closest('[role="dialog"]');
    if (dialog && e.target instanceof Node && !dialog.contains(e.target)) primary?.focus({ preventScroll: true });
  }

  function body(step: Step): string {
    if (step.id === 'notify' && target === 'settings') return tt('notify.settings');
    const k = `${step.id}.phone`;
    if (phone && hasKey(k)) return tt(k);
    return tt(`${step.id}.body` as Parameters<typeof tt>[0], { mod: modLabel() });
  }

  const last = $derived(tour.index >= tour.steps.length - 1);
</script>

<svelte:window onkeydowncapture={onKey} onfocusin={onFocus} />

{#if tour.phase === 'welcome'}
  <div class="scrim dim" role="presentation"></div>
  <div class="card welcome" class:sheet={phone} role="dialog" aria-modal="true" aria-labelledby="tour-title">
    <img src="/logo.svg" alt="" width="28" height="28" />
    <h2 id="tour-title">{tt('welcomeTitle')}</h2>
    <p>{tt('welcomeBody')}</p>
    <div class="actions">
      <button class="btn ghost" onclick={() => tour.finish()}>{tt('skip')}</button>
      <button class="btn primary" bind:this={primary} onclick={() => tour.start()}>{tt('take')}</button>
    </div>
  </div>
{:else if tour.step}
  {@const step = tour.step}
  <div class="scrim" class:dim={!rect} role="presentation"></div>
  {#if rect}
    <div class="hole" style:top="{rect.top}px" style:left="{rect.left}px" style:width="{rect.width}px" style:height="{rect.height}px"></div>
  {/if}
  <div
    class="card"
    class:sheet={phone}
    class:top={phone && below}
    class:placed={!!pos}
    bind:this={card}
    style:top={pos ? `${pos.top}px` : undefined}
    style:left={pos ? `${pos.left}px` : undefined}
    role="dialog"
    aria-modal="true"
    aria-label={tour.mode === 'whatsNew' ? tt('whatsNew') : tt('label')}
    aria-live="polite"
  >
    <div class="meta faint">
      {#if tour.mode === 'whatsNew'}<span class="new">{tt('newIn', { version: step.since })}</span>{/if}
      <span>{tt('counter', { n: tour.index + 1, total: tour.steps.length })}</span>
    </div>
    <h2>{tt(`${step.id}.title` as Parameters<typeof tt>[0])}</h2>
    <p>{body(step)}</p>
    <div class="actions">
      {#if !last}<button class="btn ghost skip" onclick={() => tour.finish()}>{tt('skip')}</button>{/if}
      <span class="spacer"></span>
      {#if tour.index > 0}<button class="btn" onclick={() => tour.back()}>{tt('back')}</button>{/if}
      <button class="btn primary" bind:this={primary} onclick={() => tour.next()}>{last ? tt('done') : tt('next')}</button>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 80;
  }
  .scrim.dim {
    background: var(--overlay);
  }
  .hole {
    position: fixed;
    z-index: 80;
    border-radius: 8px;
    pointer-events: none;
    box-shadow:
      0 0 0 2px var(--accent),
      0 0 0 200vmax var(--overlay);
  }
  .card {
    position: fixed;
    z-index: 81;
    width: 320px;
    max-width: calc(100vw - 24px);
    padding: 14px 16px 12px;
    background: var(--surface);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
    user-select: text;
    -webkit-user-select: text;
    visibility: hidden;
  }
  .card.placed,
  .card.sheet,
  .card.welcome {
    visibility: visible;
  }
  .welcome {
    top: 50%;
    left: 50%;
    width: 380px;
    transform: translate(-50%, -50%);
    padding: 20px 20px 16px;
  }
  .welcome img {
    display: block;
    margin-bottom: 12px;
    border-radius: 6px;
    box-shadow: 0 0 0 1px var(--border-strong);
  }
  .welcome h2 {
    font-size: 16px;
  }
  .meta {
    display: flex;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 11px;
    color: var(--text-3);
  }
  .new {
    color: var(--accent);
    font-weight: 600;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 600;
  }
  p {
    margin: 0 0 14px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .skip {
    margin-left: -8px;
    color: var(--text-3);
  }
  .card.sheet {
    top: auto;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    max-width: 100%;
    transform: none;
    border-radius: 14px 14px 0 0;
    padding: 16px 16px calc(14px + var(--safe-bottom));
  }
  .card.sheet.top {
    top: 0;
    bottom: auto;
    border-radius: 0 0 14px 14px;
    padding: calc(14px + var(--safe-top)) 16px 14px;
  }
  .card.sheet h2 {
    font-size: 16px;
  }
  .card.sheet p {
    font-size: 14px;
  }
  .card.sheet .btn {
    min-height: 40px;
    padding: 0 14px;
  }
  @media (prefers-reduced-motion: no-preference) {
    .hole {
      transition:
        top 180ms cubic-bezier(0.2, 0, 0, 1),
        left 180ms cubic-bezier(0.2, 0, 0, 1),
        width 180ms cubic-bezier(0.2, 0, 0, 1),
        height 180ms cubic-bezier(0.2, 0, 0, 1);
    }
    .card.placed {
      transition:
        top 180ms cubic-bezier(0.2, 0, 0, 1),
        left 180ms cubic-bezier(0.2, 0, 0, 1);
    }
  }
</style>
