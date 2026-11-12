<script lang="ts">
  import Users from '@lucide/svelte/icons/users';
  import X from '@lucide/svelte/icons/x';
  import { isTauri } from '../../daemon/client';
  import { t, type Key } from '../../i18n.svelte';
  import { app } from '../../state.svelte';
  import { saveSettings, settings, type Settings } from '../../settings/settings.svelte';
  import { mayAsk, MAX_COOLDOWN_S, NOTIFY_KINDS, opensSettings, type Delivery, type Perm } from '../../notify/notify-prefs';
  import { isTargetMuted, mutedList, openSystemSettings, permission, requestPermission, sendTest, toggleTargetMute } from '../../notify/os-notify';

  let { row = () => true, hit = () => false }: { row?: (id: string) => boolean; hit?: (id: string) => boolean } = $props();

  let perm = $state<Perm | null>(null);
  let tested = $state<boolean | null>(null);
  const deliveries: Delivery[] = ['os', 'app', 'off'];
  const toggles: [keyof Settings & `notify${string}`, Key, Key | null][] = [
    ['notifySound', 'nb.sound', null],
    ['notifyPreview', 'nb.preview', 'nb.previewHint'],
    ['notifyOnlyUnfocused', 'nb.unfocused', 'nb.unfocusedHint'],
    ['notifySkipWatched', 'nb.skipWatched', null],
    ['notifyBackgroundToasts', 'settings.notifyBackgroundToasts', null],
  ];
  const days = $derived.by(() => {
    const f = new Intl.DateTimeFormat(settings.locale, { weekday: 'short' });
    return [1, 2, 3, 4, 5, 6, 0].map((d) => ({ d, label: f.format(new Date(2024, 0, 7 + d)) }));
  });
  const canAsk = $derived(!!perm && perm !== 'granted' && mayAsk(perm, true) && (isTauri() || perm === 'prompt'));
  const groups = $derived([...app.ws.workspaces.filter((w) => !w.archived).map((w) => ({ id: `w:${w.id}`, name: w.name, group: false })), ...app.ws.groups.map((g) => ({ id: `g:${g.id}`, name: g.name, group: true }))]);
  const sessions = $derived(mutedList(app).filter((k) => !k.startsWith('w:') && !k.startsWith('g:')));

  $effect(() => {
    void app.os;
    permission(app.os).then((p) => (perm = p));
  });

  async function ask() {
    perm = await requestPermission(app.os, true);
  }

  async function test() {
    tested = await sendTest(app).catch(() => false);
    perm = await permission(app.os);
  }

  function openSystem() {
    openSystemSettings().catch((e) => app.notify(String((e as Error)?.message ?? e), 'error'));
  }

  const setKind = (k: (typeof NOTIFY_KINDS)[number], d: Delivery) => saveSettings({ notifyKinds: { ...settings.notifyKinds, [k]: d } });
  const setQuiet = (patch: Partial<Settings['notifyQuiet']>) => saveSettings({ notifyQuiet: { ...settings.notifyQuiet, ...patch } });
  const toggleDay = (d: number) => setQuiet({ days: settings.notifyQuiet.days.includes(d) ? settings.notifyQuiet.days.filter((x) => x !== d) : [...settings.notifyQuiet.days, d] });
  const sessionName = (k: string) => app.items.find((i) => i.key === k)?.title ?? k;
</script>

<section id="settings-notifications">
  <h3>{t('settings.notifications')}</h3>
  {#if row('notifyPermission')}
    <div class="row" class:hit={hit('notifyPermission')}>
      <span>{t('nb.permission')}<small>{perm ? t(`nb.perm.${perm}`) : t('common.loading')}</small></span>
      <span class="acts">
        {#if canAsk}<button class="btn" onclick={ask}>{t('nb.request')}</button>{/if}
        {#if opensSettings(isTauri(), app.os)}<button class="btn" onclick={openSystem}>{t('nb.openSettings')}</button>{/if}
        <button class="btn" onclick={test}>{t('nb.test')}</button>
      </span>
    </div>
    {#if tested !== null}<p class="faint hint" class:bad={!tested}>{t(tested ? 'nb.testSent' : 'nb.testFailed')}</p>{/if}
  {/if}

  {#if row('notifyEvents')}
    <h4>{t('nb.events')}</h4>
    {#each NOTIFY_KINDS as k (k)}
      <label class="row" class:hit={hit('notifyEvents')}>
        <span>{t(`nb.kind.${k}`)}</span>
        <select value={settings.notifyKinds[k]} onchange={(e) => setKind(k, e.currentTarget.value as Delivery)}>
          {#each deliveries as d (d)}<option value={d}>{t(`nb.d.${d}`)}</option>{/each}
        </select>
      </label>
    {/each}
  {/if}

  {#if row('notifyBehaviour')}
    <h4>{t('nb.behaviour')}</h4>
    {#each toggles as [k, label, hint] (k)}
      <label class="row" class:hit={hit('notifyBehaviour')}>
        <span>{t(label)}{#if hint}<small>{t(hint)}</small>{/if}</span>
        <input type="checkbox" class="switch" checked={!!settings[k]} onchange={(e) => saveSettings({ [k]: e.currentTarget.checked })} />
      </label>
    {/each}
    <label class="row" class:hit={hit('notifyBehaviour')}>
      <span>{t('nb.cooldown')}<small>{settings.notifyCooldown ? t('nb.cooldownHint', { n: settings.notifyCooldown }) : t('nb.cooldownOff')}</small></span>
      <input type="range" class="range" min="0" max={MAX_COOLDOWN_S} step="5" value={settings.notifyCooldown} onchange={(e) => saveSettings({ notifyCooldown: Number(e.currentTarget.value) })} />
    </label>
  {/if}

  {#if row('notifyQuiet')}
    <label class="row quiet" class:hit={hit('notifyQuiet')}>
      <span>{t('nb.quiet')}<small>{t('nb.quietHint')}</small></span>
      <input type="checkbox" class="switch" checked={settings.notifyQuiet.on} onchange={(e) => setQuiet({ on: e.currentTarget.checked })} />
    </label>
    {#if settings.notifyQuiet.on}
      <div class="row">
        <span class="times">
          <label>{t('nb.from')} <input type="time" value={settings.notifyQuiet.start} onchange={(e) => e.currentTarget.value && setQuiet({ start: e.currentTarget.value })} /></label>
          <label>{t('nb.to')} <input type="time" value={settings.notifyQuiet.end} onchange={(e) => e.currentTarget.value && setQuiet({ end: e.currentTarget.value })} /></label>
        </span>
      </div>
      <div class="days" role="group" aria-label={t('nb.days')}>
        {#each days as { d, label } (d)}
          <button class:on={settings.notifyQuiet.days.includes(d)} aria-pressed={settings.notifyQuiet.days.includes(d)} onclick={() => toggleDay(d)}>{label}</button>
        {/each}
      </div>
    {/if}
  {/if}

  {#if row('notifyMutes')}
    <h4>{t('nb.mutes')}</h4>
    <p class="faint hint">{t('nb.mutesHint')}</p>
    {#each groups as g (g.id)}
      <label class="row" class:hit={hit('notifyMutes')}>
        <span class="name">{#if g.group}<Users size={12} />{/if}{g.name}</span>
        <input type="checkbox" class="switch" checked={!isTargetMuted(app, g.id)} aria-label={t('nb.notifyFor', { name: g.name })} onchange={() => toggleTargetMute(app, g.id)} />
      </label>
    {:else}
      <p class="faint hint">{t('common.none')}</p>
    {/each}
    {#if sessions.length}
      <h4>{t('nb.mutedSessions')}</h4>
      <ul class="muted">
        {#each sessions as k (k)}
          <li>
            <span class="name">{sessionName(k)}</span>
            <button class="icon-btn" aria-label={t('nb.unmute')} title={t('nb.unmute')} onclick={() => toggleTargetMute(app, k)}><X size={14} /></button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section {
    padding: 10px 0 12px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  h4 {
    margin: 12px 0 4px;
    font-size: 12px;
    font-weight: 600;
  }
  .hint {
    margin: 0 0 4px;
    font-size: 11px;
  }
  .bad {
    color: var(--st-failed);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
  }
  .row > span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  small {
    color: var(--text-3);
    font-size: 11px;
  }
  .acts {
    flex-direction: row !important;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }
  select {
    flex: none;
    width: 130px;
  }
  .range {
    flex: none;
    width: 140px;
  }
  .times {
    flex-direction: row !important;
    flex-wrap: wrap;
    gap: 12px;
  }
  .times label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 0;
  }
  .days button {
    min-width: 40px;
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-2);
  }
  .days button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg, #fff);
  }
  .quiet {
    margin-top: 8px;
  }
  .row > .name {
    display: block;
  }
  .name :global(svg) {
    margin-right: 6px;
    vertical-align: -1px;
    color: var(--text-3);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .muted li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 30px;
  }
  :global(.phone) .row {
    min-height: var(--touch);
  }
  :global(.phone) .days button {
    min-height: var(--touch);
  }
  .row.hit {
    margin: 0 -8px;
    padding-left: 8px;
    padding-right: 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
</style>
