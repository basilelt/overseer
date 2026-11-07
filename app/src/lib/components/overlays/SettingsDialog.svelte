<script lang="ts">
  import { untrack } from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import Search from '@lucide/svelte/icons/search';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { isMac } from '../../keyboard';
  import { ui } from '../../mobile/ui.svelte';
  import Modal from '../ui/Modal.svelte';
  import AgentPicker from '../ui/AgentPicker.svelte';
  import ModelField from '../ui/ModelField.svelte';
  import UpdateSettings from './UpdateSettings.svelte';
  import ServerSettings from './ServerSettings.svelte';
  import NotificationSettings from './NotificationSettings.svelte';
  import { adaptChoice, findAgent, hasModel, taskAgents } from '../../agents';
  import { localeName, locales, t, type Key } from '../../i18n.svelte';
  import { COLOR_TOKENS, saveSettings, settings, type ColorToken, type Mode, type PresetColors, type ThemePref } from '../../settings.svelte';
  import { contrastWarnings, fullColors, presetsFor, readableOn } from '../../theme-edit';
  import { createPreset, deletePreset, editPreset, selectPreset } from '../../theme-edit.svelte';
  import { app } from '../../state.svelte';
  import { diag, dt } from '../../diag.svelte';
  import { feedback, ft } from '../../feedback.svelte';
  import { DEFAULT_REPO, PRIVACY_URL } from '../../feedback';
  import { openExternal } from '../../ports.svelte';
  import { organizer } from '../../organize.svelte';
  import { BUNDLED_FONT, clampLineHeight, FONT_SUGGESTIONS } from '../../fonts';
  import type { Effort, HookAgentStatus } from '../../types';
  import type { StatuslineStatus } from '../../usage/types';
  import { hookable } from '../../hooks';
  import { hooks, loadHooks, setHooks } from '../../hooks.svelte';
  import { loadSwitchboard, switchboard } from '../../switchboard.svelte';
  import { canImport } from '../../switchboard';
  import { tt } from '../../tour/strings';
  import { replayTour } from '../../tour/replay';
  import { clampScrollback, MAX_SCROLLBACK_LINES, MIN_SCROLLBACK_LINES } from '../../scroll';
  import { browsers } from '../../browsers.svelte';
  import { toHex6 } from '../../color-hex';
  import { updater } from '../../updater.svelte';
  import { apk } from '../../mobile/apk-update.svelte';
  import { en } from '../../i18n/en';
  import { availableCategories, availableSections, categoryCounts, escapeAction, initialCategory, isHit, phonePage, pickCategory, search, SECTIONS, showEntry, showSection, type CategoryId, type Gates, type SectionId } from '../../settings-index';

  let { onclose }: { onclose: () => void } = $props();
  const apkSettings = __SELF_UPDATE__ ? () => import('../mobile/ApkUpdateSettings.svelte') : null;

  const themes: ThemePref[] = ['auto', 'light', 'dark'];
  const themeKey: Record<ThemePref, Key> = { auto: 'settings.theme.system', light: 'settings.theme.light', dark: 'settings.theme.dark' };
  const modes: Mode[] = ['light', 'dark'];
  const presetOf = (m: Mode) => (m === 'dark' ? settings.darkPreset : settings.lightPreset);
  const customOf = (m: Mode) => settings.customPresets.find((p) => p.id === presetOf(m) && p.mode === m);
  let draft = $state<{ id: string; name: string; colors: PresetColors } | null>(null);
  const warnings = $derived(draft ? contrastWarnings(draft.colors) : []);
  let dirty = false;
  let colorTimer: ReturnType<typeof setTimeout> | undefined;
  const flushDraft = () => {
    clearTimeout(colorTimer);
    if (dirty && draft) editPreset(draft.id, { name: draft.name.trim() || undefined, colors: $state.snapshot(draft.colors) });
    dirty = false;
  };
  // WebKit's native picker can fire `input` on every drag step; saving recreates terminals, so preview live and save once settled.
  function touchDraft() {
    dirty = true;
    clearTimeout(colorTimer);
    colorTimer = setTimeout(flushDraft, 300);
  }
  function openEditor(id: string) {
    flushDraft();
    const p = settings.customPresets.find((x) => x.id === id);
    draft = p ? { id, name: p.name, colors: fullColors($state.snapshot(p)) } : null;
  }
  function closeEditor() {
    flushDraft();
    draft = null;
  }
  function choosePreset(m: Mode, id: string) {
    closeEditor();
    selectPreset(m, id);
  }
  function newPreset(m: Mode) {
    closeEditor();
    openEditor(createPreset(m, t('settings.palette.custom')));
  }
  function removePreset(id: string) {
    dirty = false;
    clearTimeout(colorTimer);
    draft = null;
    deletePreset(id);
  }
  function pickColor(k: ColorToken, v: string) {
    if (!draft) return;
    draft.colors[k] = v;
    touchDraft();
  }
  $effect(() => flushDraft);
  const agents = $derived(taskAgents(app.agents));
  const info = $derived(findAgent(app.agents, settings.defaultAgent));
  const efforts = $derived<Effort[]>(info?.efforts ?? ['low', 'medium', 'high', 'max']);

  function setAgent(id: string) {
    const next = adaptChoice(findAgent(app.agents, id), { model: settings.defaultModel, effort: settings.defaultEffort });
    saveSettings({ defaultAgent: id, defaultModel: next.model, ...(next.effort ? { defaultEffort: next.effort } : {}) });
  }

  const hookRows = $derived(hookable(hooks.status));
  function hookDetail(h: HookAgentStatus): string {
    const state = h.installed && !h.current ? t('hooks.outdated') : !h.configured && !h.installed ? t('hooks.unavailable') : '';
    return [t(`hooks.mech.${h.mechanism}`), state, h.note].filter(Boolean).join(' · ');
  }
  $effect(() => {
    untrack(() => {
      loadHooks(true);
      loadSwitchboard(true);
    });
  });

  let statusline = $state<StatuslineStatus | null>(null);
  let statuslineBusy = $state(false);
  $effect(() => {
    if (!app.connected || app.conn.phase !== 'connected') return;
    untrack(() =>
      app.api
        .statusline()
        .then((s) => (statusline = s))
        .catch(() => (statusline = null)),
    );
  });
  async function setStatusline(on: boolean) {
    statuslineBusy = true;
    try {
      statusline = await app.api.setStatusline(on);
      if (statusline.changed && statusline.backup) app.notify(t('settings.statuslineBackup', { path: statusline.backup }), 'info');
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    } finally {
      statuslineBusy = false;
    }
  }

  $effect(() => {
    if (!app.connected || app.conn.phase !== 'connected') return;
    untrack(() =>
      app.api
        .autoOrganize()
        .then((r) => saveSettings({ autoOrganize: r.enabled }))
        .catch(() => {}),
    );
  });
  async function setAutoOrganize(el: HTMLInputElement) {
    const on = el.checked;
    try {
      const r = await app.api.setAutoOrganize(on);
      saveSettings({ autoOrganize: r.enabled });
    } catch (e) {
      el.checked = !on;
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  const phoneLinks = $derived(app.mobile || ui.layout === 'phone');
  const linkValue = $derived(settings.linkTarget === 'browser' && !phoneLinks ? `browser:${settings.linkBrowser}` : settings.linkTarget === 'browser' ? 'default' : settings.linkTarget);
  $effect(() => void browsers.load(!phoneLinks));
  function setLink(v: string) {
    if (v.startsWith('browser:')) saveSettings({ linkTarget: 'browser', linkBrowser: v.slice(8) });
    else saveSettings({ linkTarget: v === 'default' ? 'default' : 'overseer' });
  }

  const phone = $derived(app.mobile || ui.layout === 'phone');
  const gates = $derived<Gates>({
    desktop: !phone,
    nonMac: !isMac,
    connected: app.connected,
    profiles: !!app.profiles,
    hooks: !!hooks.status,
    updater: updater.enabled && !app.mobile,
    apk: __SELF_UPDATE__ && apk.enabled && apk.updates,
    switchboard: canImport(switchboard.status),
  });
  const text = (k: Key) => [t(k), en[k]];
  let query = $state('');
  let cat = $state<CategoryId | null>(untrack(() => (app.mobile || ui.layout === 'phone' ? (app.overlayArg === 'notifications' ? 'notifications' : null) : initialCategory(app.overlayArg))));
  let searchEl: HTMLInputElement | undefined;
  const result = $derived(query.trim() ? search(query, gates, text) : null);
  const categories = $derived(availableCategories(gates));
  const current = $derived(phone ? (categories.some((c) => c.id === cat) ? cat : null) : pickCategory(cat, gates));
  const page = $derived(phone ? phonePage(query, current) : 'section');
  const counts = $derived(result ? categoryCounts(result) : null);
  const live = $derived(new Set(availableSections(gates).map((s) => s.id)));
  const home = Object.fromEntries(SECTIONS.map((s) => [s.id, s.category])) as Record<SectionId, CategoryId>;
  const vis = (id: SectionId) => live.has(id) && (result ? showSection(result, id) : home[id] === current);
  const row = (s: SectionId, e: string) => showEntry(result, s, e);
  const hit = (s: SectionId, e: string) => isHit(result, s, e);
  const currentTitle = $derived(categories.find((c) => c.id === current)?.title);
  let bodyEl: HTMLElement | undefined;

  function pick(id: CategoryId) {
    query = '';
    cat = id;
    bodyEl?.scrollTo?.({ top: 0 });
  }
  function onkey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    const action = escapeAction(query, phone, current);
    if (action === 'close') return;
    e.stopPropagation();
    if (action === 'clear') query = '';
    else cat = null;
    searchEl?.focus();
  }

  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Number.isFinite(v) ? v : lo));
</script>

<Modal title={t('settings.title')} {onclose} width={780}>
  <div class="settings" class:phone data-page={page} onkeydown={onkey} role="presentation">
    <nav class="side" aria-label={t('settings.title')}>
      <label class="search">
        <Search size={14} />
        <input bind:this={searchEl} bind:value={query} type="search" placeholder={t('settings.search')} aria-label={t('settings.search')} autocomplete="off" spellcheck="false" />
      </label>
      <ul class="cats">
        {#each categories as c (c.id)}
          {@const n = counts?.[c.id] ?? 0}
          <li>
            <button class="cat" class:on={!result && current === c.id} class:dim={!!result && !n} aria-current={!result && current === c.id ? 'page' : undefined} onclick={() => pick(c.id)}>
              <span>{t(c.title)}</span>
              {#if result && n}<em>{n}</em>{/if}
              {#if phone}<ChevronRight size={16} />{/if}
            </button>
          </li>
        {/each}
      </ul>
    </nav>

    <div class="body" bind:this={bodyEl}>
      {#if phone && page === 'section' && currentTitle}
        <button class="back" onclick={() => (cat = null)}><ChevronLeft size={16} />{t('settings.back')}<strong>{t(currentTitle)}</strong></button>
      {/if}
      {#if result && !result.count}
        <p class="empty" role="status">{t('settings.noResults', { query: query.trim() })}</p>
      {/if}

      {#if vis('appearance')}
        <section>
          <h3>{t('settings.appearance')}</h3>
          {#if row('appearance', 'theme')}
            <div class="row" class:hit={hit('appearance', 'theme')}>
              <span>{t('settings.theme')}</span>
              <div class="seg themes" role="radiogroup" aria-label={t('settings.theme')}>
                {#each themes as th}
                  <button role="radio" aria-checked={settings.theme === th} class:on={settings.theme === th} onclick={() => saveSettings({ theme: th })}>{t(themeKey[th])}</button>
                {/each}
              </div>
            </div>
          {/if}
          {#if row('appearance', 'language')}
            <div class="row" class:hit={hit('appearance', 'language')}>
              <span>{t('settings.language')}</span>
              <select aria-label={t('settings.language')} value={settings.locale} onchange={(e) => saveSettings({ locale: e.currentTarget.value })}>
                {#each locales as code}
                  <option value={code}>{localeName(code)}</option>
                {/each}
              </select>
            </div>
          {/if}
          {#if row('appearance', 'palettes')}
            {#each modes as m}
              <div class="row" class:hit={hit('appearance', 'palettes')}>
                <span>{t(m === 'dark' ? 'settings.palette.dark' : 'settings.palette.light')}</span>
                <div class="palette-ctl">
                  <select aria-label={t(m === 'dark' ? 'settings.palette.dark' : 'settings.palette.light')} value={presetOf(m)} onchange={(e) => choosePreset(m, e.currentTarget.value)}>
                    {#each presetsFor(m) as b}
                      <option value={b.id}>{t(b.label)}</option>
                    {/each}
                    {#each settings.customPresets.filter((p) => p.mode === m) as p (p.id)}
                      <option value={p.id}>{p.name}</option>
                    {/each}
                  </select>
                  <button class="btn" onclick={() => newPreset(m)}>{t('settings.palette.new')}</button>
                  {#if customOf(m)}
                    <button class="btn" aria-pressed={draft?.id === customOf(m)?.id} onclick={() => (draft?.id === customOf(m)?.id ? closeEditor() : openEditor(customOf(m)!.id))}>{t('review.edit')}</button>
                    <button class="btn" onclick={() => removePreset(customOf(m)!.id)}>{t('common.delete')}</button>
                  {/if}
                </div>
              </div>
            {/each}
            {#if draft}
              <div class="editor" role="group" aria-label={t('review.edit')}>
                <label class="row stacked">
                  <span>{t('ws.name')}</span>
                  <input value={draft.name} maxlength="40" spellcheck="false" oninput={(e) => { if (draft) draft.name = e.currentTarget.value; touchDraft(); }} />
                </label>
                <div class="edit-body">
                  <div class="colors">
                    {#each COLOR_TOKENS as k}
                      <label>
                        <input type="color" value={draft.colors[k]} oninput={(e) => pickColor(k, e.currentTarget.value)} />
                        <span>{t(`settings.color.${k}`)}</span>
                      </label>
                    {/each}
                  </div>
                  <div class="preview" aria-hidden="true" style:background={draft.colors.bg} style:color={draft.colors.text} style:border-color={draft.colors.border}>
                    <strong>{draft.name}</strong>
                    <span style:color={draft.colors['text-2']}>{t('settings.color.text-2')}</span>
                    <div class="card" style:background={draft.colors.surface} style:border-color={draft.colors.border}>
                      <span>{t('settings.color.surface')}</span>
                      <button class="accent" tabindex="-1" style:background={draft.colors.accent} style:color={readableOn(draft.colors.accent)}>{t('settings.color.accent')}</button>
                    </div>
                  </div>
                </div>
                {#each warnings as w}
                  <p class="warn" role="status">{t('settings.palette.lowContrast', { fg: t(`settings.color.${w.fg}`), bg: t(`settings.color.${w.bg}`), ratio: w.ratio.toFixed(1) })}</p>
                {/each}
                <button class="btn" onclick={closeEditor}>{t('tour.done')}</button>
              </div>
            {/if}
          {/if}
        </section>
      {/if}

      {#if vis('terminal')}
        <section>
          <h3>{t('settings.terminal')}</h3>
          {#if row('terminal', 'fontFamily')}
            <label class="row stacked" class:hit={hit('terminal', 'fontFamily')}>
              <span>{t('settings.fontFamily')}<small>{t('settings.fontFamilyHint')}</small></span>
              <input class="mono" list="term-fonts" value={settings.terminalFont} placeholder={BUNDLED_FONT} spellcheck="false" onchange={(e) => saveSettings({ terminalFont: e.currentTarget.value.trim() })} />
              <datalist id="term-fonts">
                {#each FONT_SUGGESTIONS as f}<option value={f}></option>{/each}
              </datalist>
            </label>
          {/if}
          {#if row('terminal', 'fontSize')}
            <label class="row" class:hit={hit('terminal', 'fontSize')}>
              <span>{t('settings.fontSize')}</span>
              <input type="number" min="9" max="24" value={settings.fontSize} onchange={(e) => saveSettings({ fontSize: clamp(e.currentTarget.valueAsNumber, 9, 24) })} />
            </label>
          {/if}
          {#if row('terminal', 'lineHeight')}
            <label class="row" class:hit={hit('terminal', 'lineHeight')}>
              <span>{t('settings.lineHeight')}</span>
              <input type="number" min="1" max="2" step="0.05" value={settings.lineHeight} onchange={(e) => saveSettings({ lineHeight: clampLineHeight(e.currentTarget.valueAsNumber) })} />
            </label>
          {/if}
          {#if row('terminal', 'scrollback')}
            <label class="row" class:hit={hit('terminal', 'scrollback')}>
              <span>{t('settings.scrollback')}<small>{t('settings.scrollbackHint')}</small></span>
              <input type="number" min={MIN_SCROLLBACK_LINES} max={MAX_SCROLLBACK_LINES} step="1000" value={settings.scrollbackLines} onchange={(e) => saveSettings({ scrollbackLines: clampScrollback(e.currentTarget.valueAsNumber) })} />
            </label>
          {/if}
          {#if row('terminal', 'gpuTerminal')}
            <label class="row" class:hit={hit('terminal', 'gpuTerminal')}>
              <span>{t('settings.gpuTerminal')}<small>{t('settings.gpuTerminalHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.gpuTerminal} onchange={(e) => saveSettings({ gpuTerminal: e.currentTarget.checked })} />
            </label>
          {/if}
          {#if row('terminal', 'shiftEnter')}
            <label class="row" class:hit={hit('terminal', 'shiftEnter')}>
              <span>{t('settings.shiftEnterNewline')}<small>{t('settings.shiftEnterNewlineHint')} <code>bindkey '^[^M' self-insert-unmeta</code></small></span>
              <input type="checkbox" class="switch" checked={settings.shiftEnterNewline} onchange={(e) => saveSettings({ shiftEnterNewline: e.currentTarget.checked })} />
            </label>
          {/if}
        </section>
      {/if}

      {#if vis('keyboard')}
        <section>
          <h3 class="with-action">
            {t('settings.keyboard')}
            <button class="icon-btn" aria-label={t('keys.title')} title={t('keys.title')} onclick={() => app.open('shortcuts')}>?</button>
          </h3>
          {#if !isMac && row('keyboard', 'ctrlW')}
            <label class="row" class:hit={hit('keyboard', 'ctrlW')}>
              <span>{t('settings.ctrlWClosesTab')}<small>{t('settings.ctrlWClosesTabHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.ctrlWClosesTab} onchange={(e) => saveSettings({ ctrlWClosesTab: e.currentTarget.checked })} />
            </label>
          {/if}
          {#if row('keyboard', 'newTabShell')}
            <label class="row" class:hit={hit('keyboard', 'newTabShell')}>
              <span>{t('settings.newTabShell')}<small>{t('settings.newTabShellHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.newTabShell} onchange={(e) => saveSettings({ newTabShell: e.currentTarget.checked })} />
            </label>
          {/if}
          {#if row('keyboard', 'wrapTabs')}
            <label class="row" class:hit={hit('keyboard', 'wrapTabs')}>
              <span>{t('settings.wrapTabs')}<small>{t('settings.wrapTabsHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.wrapTabs} onchange={(e) => saveSettings({ wrapTabs: e.currentTarget.checked })} />
            </label>
          {/if}
        </section>
      {/if}

      {#if vis('links')}
        <section>
          <h3>{t('settings.links')}</h3>
          {#if row('links', 'linkTarget')}
            <label class="row" class:hit={hit('links', 'linkTarget')}>
              <span>{t('settings.linkTarget')}{#if !phoneLinks}<small>{t('settings.linkTargetHint', { mod: isMac ? '⌘' : 'Ctrl' })}</small>{/if}</span>
              <select value={linkValue} onchange={(e) => setLink(e.currentTarget.value)}>
                <option value="overseer">{t('settings.linkTarget.overseer')}</option>
                <option value="default">{t('settings.linkTarget.default')}</option>
                {#if !phoneLinks}
                  {#each browsers.list ?? [] as b (b.id)}<option value="browser:{b.id}">{b.name}</option>{/each}
                  {#if settings.linkTarget === 'browser' && !browsers.list?.some((b) => b.id === settings.linkBrowser)}<option value="browser:{settings.linkBrowser}">{settings.linkBrowser}</option>{/if}
                {/if}
              </select>
            </label>
          {/if}
          {#if row('links', 'linkAsk')}
            <label class="row" class:hit={hit('links', 'linkAsk')}>
              <span>{t('settings.linkAsk')}<small>{t('settings.linkAskHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.linkAsk} onchange={(e) => saveSettings({ linkAsk: e.currentTarget.checked })} />
            </label>
          {/if}
        </section>
      {/if}

      {#if vis('defaults')}
        <section>
          <h3>{t('settings.defaults')}</h3>
          {#if row('defaults', 'defaultAgent')}
            <div class="row" class:hit={hit('defaults', 'defaultAgent')}>
              <span>{t('settings.defaultAgent')}</span>
              <div class="ctl">
                <AgentPicker {agents} value={settings.defaultAgent} label={t('settings.defaultAgent')} onchange={setAgent} />
              </div>
            </div>
          {/if}
          {#if hasModel(info) && row('defaults', 'defaultModel')}
            <div class="row" class:hit={hit('defaults', 'defaultModel')}>
              <span>{t('settings.defaultModel')}</span>
              <div class="ctl">
                <ModelField agent={info} value={settings.defaultModel} label={t('settings.defaultModel')} onchange={(v) => saveSettings({ defaultModel: v })} />
              </div>
            </div>
          {/if}
          {#if efforts.length && row('defaults', 'defaultEffort')}
            <div class="row" class:hit={hit('defaults', 'defaultEffort')}>
              <span>{t('settings.defaultEffort')}</span>
              <div class="seg" role="radiogroup" aria-label={t('settings.defaultEffort')}>
                {#each efforts as e}
                  <button role="radio" aria-checked={settings.defaultEffort === e} class:on={settings.defaultEffort === e} onclick={() => saveSettings({ defaultEffort: e })}>{t(`effort.${e}`)}</button>
                {/each}
              </div>
            </div>
          {/if}
          {#if row('defaults', 'autoConcurrency')}
            <label class="row" class:hit={hit('defaults', 'autoConcurrency')}>
              <span>{t('settings.autoConcurrency')}<small>{t('settings.autoConcurrencyHint')}</small></span>
              <input type="number" min="1" max="16" value={settings.autoConcurrency} onchange={(e) => saveSettings({ autoConcurrency: clamp(e.currentTarget.valueAsNumber, 1, 16) })} />
            </label>
          {/if}
          {#if row('defaults', 'autoOrganize')}
            <label class="row" class:hit={hit('defaults', 'autoOrganize')}>
              <span>{t('settings.autoOrganize')}<small>{t('settings.autoOrganizeHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.autoOrganize} onchange={(e) => setAutoOrganize(e.currentTarget)} />
            </label>
          {/if}
        </section>
      {/if}

      {#if vis('profiles') && app.profiles}
        <section>
          <h3>{t('profile.title')}</h3>
          {#if row('profiles', 'profileDefault')}
            <label class="row" class:hit={hit('profiles', 'profileDefault')}>
              <span>{t('profile.globalDefault')}<small>{t('profile.globalHint')}</small></span>
              <select
                class="ctl"
                value={app.profiles.default || 'none'}
                onchange={(e) => app.saveProfiles({ default: e.currentTarget.value === 'none' ? '' : e.currentTarget.value }).catch((err) => app.notify(String(err?.message ?? err), 'error'))}
              >
                <option value="none">{t('profile.none')}</option>
                {#each app.profiles.profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </label>
          {/if}
          {#if row('profiles', 'profileEdit')}
            <div class="row" class:hit={hit('profiles', 'profileEdit')}>
              <span>{t('profile.editTitle')}<small>{t('profile.editHint')}</small></span>
              <button class="btn" onclick={() => app.open('profiles')}>{t('profile.edit')}</button>
            </div>
          {/if}
        </section>
      {/if}

      {#if vis('hooks') && hooks.status}
        <section>
          <h3>{t('hooks.title')}</h3>
          <p class="faint hint">{t('hooks.hint')}</p>
          {#each hookRows as h (h.agent)}
            <label class="row">
              <span>{h.name}<small>{hookDetail(h)}</small></span>
              <input
                type="checkbox"
                class="switch"
                checked={h.installed}
                disabled={hooks.busy || (!h.configured && !h.installed)}
                title={h.path}
                onchange={(e) => setHooks([h.agent], e.currentTarget.checked)}
              />
            </label>
          {/each}
          {#each hooks.status.agents.filter((a) => a.mechanism === 'polling') as h (h.agent)}
            <div class="row">
              <span>{h.name}<small>{t('hooks.mech.polling')}</small></span>
            </div>
          {/each}
        </section>
      {/if}

      {#if vis('usage')}
        <section>
          <h3>{t('settings.usage')}</h3>
          {#if row('usage', 'claudeLimits')}
            <label class="row" class:hit={hit('usage', 'claudeLimits')}>
              <span>{t('settings.fetchClaudeLimits')}<small>{t('settings.fetchClaudeLimitsHint')}</small></span>
              <input type="checkbox" class="switch" checked={settings.fetchClaudeLimits} onchange={(e) => saveSettings({ fetchClaudeLimits: e.currentTarget.checked })} />
            </label>
          {/if}
          {#if statusline && row('usage', 'statusline')}
            <label class="row" class:hit={hit('usage', 'statusline')}>
              <span
                >{t('settings.statusline')}<small
                  >{t('settings.statuslineHint')}{statusline.command ? ` ${t('settings.statuslineWraps', { cmd: statusline.command })}` : ''}</small
                ></span
              >
              <input type="checkbox" class="switch" checked={statusline.installed} disabled={statuslineBusy} title={statusline.path} onchange={(e) => setStatusline(e.currentTarget.checked)} />
            </label>
          {/if}
        </section>
      {/if}

      {#if vis('notifications')}<NotificationSettings row={(e) => row('notifications', e)} hit={(e) => hit('notifications', e)} />{/if}

      {#if vis('hosts')}
        <section>
          <h3>{t('settings.hosts')}</h3>
          {#if row('hosts', 'hosts')}
            {#if settings.recentHosts.length}
              <ul class="hosts">
                {#each settings.recentHosts as h (h.host)}
                  <li>
                    <span class="mono">{h.host}</span>
                    <button class="icon-btn" aria-label={t('settings.removeHost')} title={t('settings.removeHost')} onclick={() => saveSettings({ recentHosts: settings.recentHosts.filter((x) => x.host !== h.host) })}><X size={14} /></button>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="faint">{t('common.none')}</p>
            {/if}
          {/if}
          {#if row('hosts', 'serverBinDir')}
            <label class="row stacked" class:hit={hit('hosts', 'serverBinDir')}>
              <span>{t('settings.serverBinDir')}<small>{t('settings.serverBinDirHint')}</small></span>
              <input class="mono" value={settings.serverBinDir} placeholder="~/overseer/dist" spellcheck="false" onchange={(e) => saveSettings({ serverBinDir: e.currentTarget.value.trim() })} />
            </label>
          {/if}
        </section>
      {/if}

      {#if vis('server')}<ServerSettings row={(e) => row('server', e)} hit={(e) => hit('server', e)} />{/if}
      {#if vis('updater')}<UpdateSettings row={(e) => row('updater', e)} hit={(e) => hit('updater', e)} />{/if}
      {#if apkSettings && vis('apk')}{#await apkSettings() then { default: ApkUpdateSettings }}<ApkUpdateSettings row={(e) => row('apk', e)} hit={(e) => hit('apk', e)} />{/await}{/if}

      {#if vis('tour')}
        <section>
          <h3>{tt('section')}</h3>
          {#if row('tour', 'tour')}
            <div class="row" class:hit={hit('tour', 'tour')}>
              <span>{tt('again')}<small>{tt('againHint')}</small></span>
              <button class="btn" onclick={() => replayTour()}>{tt('take')}</button>
            </div>
          {/if}
          {#if row('tour', 'organize')}
            <div class="row" class:hit={hit('tour', 'organize')}>
              <span>{t('org.title')}<small>{t('org.helpHint')}</small></span>
              <button class="btn" disabled={!app.connected} onclick={() => (onclose(), organizer.show())}>{t('org.nudgeAction')}</button>
            </div>
          {/if}
        </section>
      {/if}

      {#if vis('diag')}
        <section>
          <h3>{dt('section')}</h3>
          <div class="row" class:hit={hit('diag', 'diag')}>
            <span>{dt('show')}<small>{dt('sectionHint')}</small></span>
            <button class="btn" onclick={() => diag.open('live')}>{dt('show')}</button>
          </div>
        </section>
      {/if}

      {#if vis('switchboard')}
        <section>
          <h3>{t('switchboard.section')}</h3>
          <div class="row" class:hit={hit('switchboard', 'switchboard')}>
            <span>{t('switchboard.title')}<small>{t('switchboard.settingsHint')}</small></span>
            <button class="btn" data-tour="switchboard" onclick={() => app.open('switchboardImport', 'settings')}>{t('switchboard.run')}</button>
          </div>
        </section>
      {/if}

      {#if vis('feedback')}
        <section>
          <h3>{ft('section')}</h3>
          <div class="row" class:hit={hit('feedback', 'feedback')}>
            <span>{ft('title')}<small>{ft('sectionHint')}</small></span>
            <button class="btn" onclick={() => feedback.show()}>{ft('button')}</button>
          </div>
          <label class="row stacked" class:hit={hit('feedback', 'feedback')}>
            <span>{ft('repo')}<small>{ft('repoHint')}</small></span>
            <input class="mono" value={settings.feedbackRepo ?? ''} placeholder={DEFAULT_REPO} spellcheck="false" onchange={(e) => saveSettings({ feedbackRepo: e.currentTarget.value.trim() || undefined })} />
          </label>
          <div class="row" class:hit={hit('feedback', 'privacy')}>
            <span>{ft('privacy')}<small>{ft('privacyHint')}</small></span>
            <button class="btn" onclick={() => openExternal(PRIVACY_URL).catch((e) => app.notify(String(e?.message ?? e), 'error'))}>{ft('openLink')}</button>
          </div>
        </section>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .settings {
    display: grid;
    grid-template-columns: 188px minmax(0, 1fr);
    height: min(520px, calc(76vh - 64px));
    margin: 0 -16px -16px;
    border-top: 1px solid var(--border);
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
    overflow: auto;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-3);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    box-shadow: none;
    outline: none;
    padding: 6px 0;
  }
  .cats {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .cat {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    border-radius: var(--radius);
    text-align: left;
    color: var(--text-2);
  }
  .cat:hover {
    background: var(--surface);
  }
  .cat.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .cat.dim {
    opacity: 0.45;
  }
  .cat em {
    font-style: normal;
    font-size: 11px;
    color: var(--accent);
  }
  .body {
    min-width: 0;
    padding: 4px 16px 16px;
    overflow: auto;
  }
  .empty {
    margin: 24px 0;
    text-align: center;
    color: var(--text-3);
  }
  .row.hit {
    margin: 0 -8px;
    padding: 0 8px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .back {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 8px 0;
    color: var(--text-2);
  }
  .back strong {
    margin-left: auto;
    color: var(--text);
  }
  .settings.phone {
    display: flex;
    flex-direction: column;
    height: calc(90dvh - 72px);
    margin: 0 -16px;
  }
  .settings.phone .side {
    flex: none;
    border-right: 0;
    background: none;
    overflow: visible;
  }
  .settings.phone[data-page='list'] .side {
    flex: 1;
    overflow: auto;
  }
  .settings.phone .body {
    flex: 1;
    min-height: 0;
  }
  .settings.phone[data-page='list'] .body,
  .settings.phone[data-page='section'] .side {
    display: none;
  }
  .settings.phone[data-page='results'] .cats {
    display: none;
  }
  .settings.phone .cat {
    padding: 0 10px;
    min-height: var(--touch);
  }
  .settings.phone .search {
    min-height: var(--touch);
  }
  .colors {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 6px 12px;
    padding: 4px 0 8px;
  }
  .colors label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .colors input {
    flex: none;
    width: 28px;
    height: 22px;
    min-height: 0;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: none;
    cursor: pointer;
  }
  section {
    padding: 10px 0 12px;
    border-bottom: 1px solid var(--border);
  }
  section:last-child {
    border-bottom: 0;
  }
  .hint {
    margin: 0 0 4px;
    font-size: 11px;
  }
  h3.with-action {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 .icon-btn {
    font-size: 12px;
    font-weight: 600;
    width: 22px;
    height: 22px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 36px;
  }
  .row.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    margin-top: 8px;
  }
  .row > span {
    display: flex;
    flex-direction: column;
  }
  small {
    color: var(--text-3);
    font-size: 11px;
  }
  code {
    font-family: var(--mono);
    font-size: 10.5px;
  }
  input[type='number'] {
    width: 72px;
  }
  .ctl {
    width: 220px;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
  }
  .themes {
    flex-wrap: wrap;
  }
  .palette-ctl {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
  }
  .edit-body {
    align-self: stretch;
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px 20px;
  }
  .edit-body .colors {
    grid-template-columns: 1fr;
    min-width: 170px;
  }
  .edit-body .preview {
    flex: 1;
    min-width: 200px;
    align-self: flex-start;
  }
  .editor {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 0 10px;
  }
  .editor .warn {
    margin: 0;
    font-size: 12px;
    color: var(--st-paused);
  }
  .preview {
    align-self: stretch;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid;
    border-radius: var(--radius-lg);
    font-size: 13px;
  }
  .preview .card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid;
    border-radius: var(--radius);
  }
  .preview .accent {
    padding: 3px 10px;
    border: 0;
    border-radius: var(--radius);
    font: inherit;
  }
  .themes button {
    white-space: nowrap;
  }
  .seg button {
    padding: 3px 10px;
    border-radius: 4px;
    color: var(--text-2);
    transition:
      background var(--ease),
      color var(--ease);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 0 0 1px var(--border);
  }
  .hosts {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .hosts li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 30px;
  }
  :global(.phone) .row {
    min-height: var(--touch);
  }
</style>
