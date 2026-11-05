<script lang="ts">
  import { untrack } from 'svelte';
  import GitPullRequest from '@lucide/svelte/icons/git-pull-request';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Bot from '@lucide/svelte/icons/bot';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Inbox from '@lucide/svelte/icons/inbox';
  import type { TabInfo } from '../../tabs';
  import { app } from '../../state.svelte';
  import { age } from '../../time';
  import { openExternal } from '../../ports.svelte';
  import { gt } from '../../github/strings';
  import { ghError, groupByRepo, reviewState, sizeLabel, sizeTone } from '../../github/format';
  import { ReviewFeed } from '../../github/feed.svelte';
  import { hostKey } from '../../window';
  import { startReview } from '../../github/open.svelte';
  import type { ReviewRequest } from '../../github/types';

  let { focused }: { tabKey: string; tab: TabInfo; focused: boolean } = $props();

  const feed = new ReviewFeed();
  const host = $derived(hostKey(app.conn.target));
  const data = $derived(feed.data);
  const error = $derived(feed.error);
  const loading = $derived(feed.loading);
  let busy = $state<string | null>(null);
  const groups = $derived(groupByRepo(data?.prs ?? []));

  async function load(refresh = false) {
    const client = app.client;
    if (!client) return;
    const failed = await feed.load(client, host, refresh);
    if (failed) app.notify(`${gt('failedTitle')}: ${failed.message}`, 'error');
  }

  $effect(() => {
    host;
    if (app.connected) untrack(() => void load());
  });

  $effect(() => {
    if (!focused || !app.connected) return;
    const onFocus = () => document.visibilityState === 'visible' && feed.due() && void load();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  });

  async function review(pr: ReviewRequest) {
    const id = `${pr.repo}#${pr.number}`;
    busy = id;
    try {
      await startReview(pr);
    } catch (e) {
      app.notify(gt('startFailed', { error: ghError(e).message }), 'error');
    } finally {
      busy = null;
    }
  }
</script>

<div class="host">
  <div class="inner">
    <header>
      <div class="titles">
        <h2><GitPullRequest size={18} />{gt('title')}</h2>
        <span class="faint">{data?.login ? gt('subtitle', { login: `@${data.login}` }) : gt('subtitleAnon')}</span>
      </div>
      {#if data?.fetchedAt}<span class="faint small">{gt('updated', { t: age(new Date(data.fetchedAt).toISOString()) })}</span>{/if}
      <button class="btn ghost" disabled={loading || !app.connected} onclick={() => load(true)}><RefreshCw size={13} class={loading ? 'spin' : ''} />{gt('refresh')}</button>
    </header>

    {#if error && !data?.prs.length}
      <div class="state">
        <TriangleAlert size={28} strokeWidth={1.5} />
        <p class="head">{gt(error.code === 'auth' ? 'authTitle' : error.code === 'missing' ? 'missingTitle' : 'failedTitle')}</p>
        {#if error.code === 'auth' || error.code === 'missing'}
          <p class="muted">{gt(error.code === 'auth' ? 'authBody' : 'missingBody')}</p>
          {#if error.code === 'missing'}<code>{error.command}</code>{/if}
          <code>gh auth login</code>
        {:else}
          <p class="muted mono small">{error.message}</p>
        {/if}
      </div>
    {:else if data && !groups.length}
      <div class="state">
        <Inbox size={28} strokeWidth={1.5} />
        <p class="head">{gt('emptyTitle')}</p>
        <p class="muted">{gt('emptyBody')}</p>
      </div>
    {:else}
      {#each groups as g (g.repo)}
        <section>
          <h3><span class="mono">{g.repo}</span><span class="count">{g.prs.length}</span></h3>
          <ul>
            {#each g.prs as pr (pr.number)}
              {@const st = reviewState(pr)}
              <li>
                <div class="main">
                  <div class="line1">
                    <span class="num faint mono">#{pr.number}</span>
                    <a class="title" href={pr.url} onclick={(e) => (e.preventDefault(), void openExternal(pr.url))}>{pr.title}</a>
                  </div>
                  <div class="line2 faint">
                    {#if pr.author}<span>@{pr.author}</span>{/if}
                    <span>{age(pr.updatedAt)}</span>
                    <span class="size {sizeTone(pr)} mono" title={gt('files', { n: pr.changedFiles })}>{sizeLabel(pr)}</span>
                    <span class="badge {st}">{gt(st)}</span>
                    {#if pr.localPath}<span class="local" title={pr.localPath}>{gt('local')}</span>{/if}
                  </div>
                </div>
                <div class="actions">
                  <button class="btn primary" disabled={!!busy || !app.connected} onclick={() => review(pr)}><Bot size={13} />{busy === `${pr.repo}#${pr.number}` ? gt('reviewing') : gt('review')}</button>
                  <button class="icon-btn" title={gt('openGithub')} aria-label={gt('openGithub')} onclick={() => openExternal(pr.url)}><ExternalLink size={15} /></button>
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>
</div>

<style>
  .host {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--bg);
  }
  .inner {
    max-width: 1000px;
    margin: 0 auto;
    padding: 16px 20px 32px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .small {
    font-size: 12px;
  }
  section {
    margin-bottom: 18px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
  .count {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--surface-hover);
    color: var(--text-3);
    font-weight: 500;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    overflow: hidden;
  }
  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .line1 {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .title {
    color: var(--text);
    text-decoration: none;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title:hover {
    text-decoration: underline;
  }
  .line2 {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    font-size: 12px;
  }
  .size.xs,
  .size.s {
    color: var(--diff-add);
  }
  .size.l,
  .size.xl {
    color: var(--diff-del);
  }
  .badge,
  .local {
    padding: 1px 7px;
    border-radius: 9px;
    font-size: 11px;
    font-weight: 500;
    background: var(--surface-hover);
    color: var(--text-2);
  }
  .badge.approved {
    background: var(--diff-add-bg);
    color: var(--diff-add);
  }
  .badge.changes {
    background: var(--diff-del-bg);
    color: var(--diff-del);
  }
  .badge.required {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 48px 16px;
    text-align: center;
    color: var(--text-3);
  }
  .state p {
    margin: 0;
  }
  .state .head {
    color: var(--text);
    font-weight: 600;
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--surface-hover);
    color: var(--text);
    user-select: all;
    -webkit-user-select: all;
  }
  :global(html.phone) li {
    flex-direction: column;
    align-items: stretch;
  }
  :global(html.phone) .actions {
    justify-content: flex-end;
  }
</style>
