<script lang="ts" module>
  import { SvelteMap } from 'svelte/reactivity';

  const owners = new SvelteMap<string, number>();
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import SquareTerminal from '@lucide/svelte/icons/square-terminal';
  import { t } from '../../i18n.svelte';
  import { customColors, resolvedTheme } from '../../settings.svelte';
  import { app } from '../../state.svelte';
  import { focusPane, update, type Pane } from '../../workspace.svelte';
  import TabBar from './TabBar.svelte';
  import TerminalPane from './TerminalPane.svelte';
  import GitCompare from '@lucide/svelte/icons/git-compare';
  import { renameTab } from '../../workspace.svelte';
  import { newTabKey, TAB_KINDS, tabInfo } from '../../tabs';
  import { paneAction } from '../../workspaces';
  import { modeOf } from '../../chat/viewmode.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import { pages } from '../../session-pages.svelte';
  import Boundary from '../ui/Boundary.svelte';
  import { claim, KEEP_TERMS, KEEP_TERMS_PHONE, retain, same, type Claim } from '../../keepalive';

  const browser = () => import('../browser/BrowserView.svelte');
  const emptyPane = () => import('./EmptyPane.svelte');
  const chatView = () => import('../chat/ChatView.svelte');
  const sessionWeb = () => import('../browser/SessionWeb.svelte');
  const review = () => TAB_KINDS.review?.load?.() ?? Promise.resolve(null);

  let { pane, index, focused, canClose }: { pane: Pane; index: number; focused: boolean; canClose: boolean } = $props();

  const item = $derived(pane.active ? app.items.find((i) => i.key === pane.active) : undefined);
  const tab = $derived(pane.active ? tabInfo(pane.active, app.items, modeOf) : null);
  const webKey = $derived(item && pages.byKey[item.key]?.open ? item.key : null);
  const phone = $derived(ui.layout === 'phone');
  const line = $derived(item?.lineId ? app.data.lines.find((l) => l.line.id === item.lineId) : undefined);
  const action = $derived(item ? paneAction(item, line) : null);
  const running = $derived(action === 'attach');

  function browsed(url: string) {
    const from = pane.active;
    if (!from?.startsWith('browser:')) return;
    const to = newTabKey('browser', url);
    if (to === from) return;
    update((l) => renameTab(l, from, to));
    if (app.selectedKey === from) app.selectedKey = to;
  }
  let resuming = $state(false);

  const activeTerm = $derived(item?.termId && !(item.lineId && tab?.kind === 'chat') ? item.termId : null);
  const look = $derived(`${resolvedTheme()}:${customColors().bg}:${customColors().text}`);
  let kept = $state<string[]>([]);
  let keptLook = '';
  let claimed: Claim | null = null;

  $effect(() => {
    const active = activeTerm;
    const at = index;
    const l = look;
    const live = new Set(app.data.terms.filter((x) => !x.exited).map((x) => x.id));
    untrack(() => {
      const next = { active, index: at };
      claim(owners, claimed, next, live);
      claimed = next;
    });
    const mine = new Set([...owners].filter(([id, i]) => i === at && live.has(id)).map(([id]) => id));
    untrack(() => {
      const base = l === keptLook ? kept : [];
      keptLook = l;
      const next = retain(base, active, (id) => mine.has(id), ui.layout === 'phone' ? KEEP_TERMS_PHONE : KEEP_TERMS);
      if (!same(next, kept)) kept = next;
    });
  });

  const stack = $derived([...kept].sort());

  async function resume() {
    if (!item) return;
    resuming = true;
    try {
      await (running ? app.openTerm(item) : app.resumeLine(item));
    } catch (e) {
      app.notify(String((e as Error).message ?? e), 'error');
    } finally {
      resuming = false;
    }
  }

  function focus() {
    if (focused) return;
    update((l) => focusPane(l, index));
    if (pane.active) app.select(pane.active);
  }
</script>

<section class="pane" class:focused role="group" onpointerdown={focus}>
  <TabBar {pane} {index} {focused} {canClose} />
  <div class="body">
    <div class="main" class:hidden={!!webKey && phone}>
      {#if tab?.kind === 'browser'}
        <Boundary name="browser">{#await browser() then { default: BrowserView }}<BrowserView initialUrl={tab.arg} onurl={browsed} />{/await}</Boundary>
      {:else if tab?.kind === 'review'}
        {#await review() then ReviewTab}
          {#if ReviewTab}
            <Boundary name="review"><ReviewTab tabKey={tab.key} {tab} {focused} /></Boundary>
          {:else}
            <div class="empty">
              <GitCompare size={28} strokeWidth={1.5} />
              <p class="muted">{t('tabs.reviewSoon')}</p>
            </div>
          {/if}
        {/await}
      {:else if tab && (tab.kind === 'graph' || tab.kind === 'worktrees' || tab.kind === 'system' || tab.kind === 'usage' || tab.kind === 'prs')}
        {#key tab.kind}
          {#await TAB_KINDS[tab.kind]?.load?.() then View}
            {#if View}<Boundary name={tab.kind}><View tabKey={tab.key} {tab} {focused} /></Boundary>{/if}
          {/await}
        {/key}
      {:else if item?.lineId && tab?.kind === 'chat'}
        <Boundary name="chat">{#await chatView() then { default: ChatView }}<ChatView {item} phone={ui.layout === 'phone'} />{/await}</Boundary>
      {:else if item?.lineId && !item.termId}
        <div class="empty">
          <SquareTerminal size={28} strokeWidth={1.5} />
          <p class="muted">{item.title}</p>
          {#if action === 'external'}
            <p class="faint">{t('term.external', { pid: line?.local?.pid ?? '' })}</p>
          {:else}
            <button class="btn primary" disabled={resuming} onclick={resume}><RotateCcw size={13} />{t(running ? 'line.attach' : 'term.resume')}</button>
          {/if}
        </div>
      {:else if !item}
        {#await emptyPane() then { default: EmptyPane }}<EmptyPane />{/await}
      {:else if !item.termId}
        <div class="empty">
          <SquareTerminal size={28} strokeWidth={1.5} />
          <p class="faint">{t('term.empty')}</p>
        </div>
      {/if}
      {#each stack as id (id)}
        <div class="keep" class:hidden={id !== activeTerm}>
          {#key `${id}:${look}`}
            <Boundary name="terminal"><TerminalPane termId={id} focused={focused && id === activeTerm} /></Boundary>
          {/key}
        </div>
      {/each}
    </div>
    {#if webKey}
      {#await sessionWeb() then { default: SessionWeb }}<SessionWeb sessionKey={webKey} {phone} />{/await}
    {/if}
  </div>
</section>

<style>
  .pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--term-bg);
  }
  .body {
    flex: 1;
    display: flex;
    min-width: 0;
    min-height: 0;
  }
  .main {
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main.hidden {
    display: none;
  }
  .keep {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .keep.hidden {
    display: none;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-3);
  }
  .empty p {
    margin: 0;
    max-width: 420px;
    padding: 0 16px;
    text-align: center;
  }
</style>
