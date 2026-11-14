<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Plus from '@lucide/svelte/icons/plus';
  import UnfoldVertical from '@lucide/svelte/icons/unfold-vertical';
  import { t } from '../../i18n.svelte';
  import * as D from '../../review/draft';
  import { highlightHunks, type Highlighted } from '../../review/highlight';
  import { buildRows, linesFor, openHunks, placeThreads, type Cell, type FoldRow, type Row } from '../../review/rows';
  import { TAG_COLOR } from '../../review/triage';
  import type { Key } from '../../i18n.svelte';
  import type { ReviewStore } from '../../review/store.svelte';
  import type { ReviewFile, Side, TagKind } from '../../review/types';
  import CommentBox from './CommentBox.svelte';
  import Modal from '../ui/Modal.svelte';
  import { isPhone } from '../../mobile/ui.svelte';
  import Thread from './Thread.svelte';
  import Lazy from '../ui/Lazy.svelte';
  import { chunks, watchNear } from '../../review/window';
  import Boxes from '@lucide/svelte/icons/boxes';
  import { saveBase } from '../../review/base';
  import { openReviewNear } from '../../review/open';
  import { reviewKey } from '../../review/keys';

  let { store, file }: { store: ReviewStore; file: ReviewFile } = $props();

  const STATUS: Record<string, string> = { added: 'A', modified: 'M', deleted: 'D', renamed: 'R', copied: 'C', untracked: 'U', submodule: 'S' };

  const expanded = $derived(new Set(store.expanded[file.path] ?? []));
  const fileLines = $derived(store.fileLines[file.path]);
  const phone = $derived(isPhone());
  const mode = $derived(phone ? 'unified' : store.mode);
  const threads = $derived(store.threads.filter((x) => x.path === file.path));
  const draft = $derived(store.draft?.path === file.path ? store.draft : null);
  const open = $derived(
    openHunks(file, store.unfolded[file.path], [...threads.filter((x) => !x.outdated).map((x) => ({ side: x.side, line: x.endLine })), ...(draft ? [{ side: draft.side, line: draft.end }] : [])]),
  );
  const rows = $derived(buildRows(file, mode, expanded, fileLines, open));
  const placed = $derived(placeThreads(rows, threads));
  const collapsed = $derived(store.isCollapsed(file.path));
  const draftRow = $derived.by(() => {
    if (!draft || draft.selecting) return -1;
    return rows.findIndex((r) => rowHas(r, draft.side, draft.end));
  });
  const split = $derived(mode === 'split');
  const CHUNK = 50;
  const chunked = $derived.by(() => {
    const line = phone ? 24 : 20;
    const gap = phone ? 36 : 28;
    return chunks(rows.length, CHUNK, (i) => {
      const r = rows[i];
      return r.type === 'fold' ? gap : r.type !== 'gap' ? line : r.count !== 0 || r.header ? gap : 0;
    });
  });
  const pinned = $derived(new Set([...placed.byRow.keys(), draftRow].filter((i) => i >= 0).map((i) => Math.floor(i / CHUNK))));

  let section: HTMLElement | undefined = $state();
  let near = $state(false);
  $effect(() => {
    if (!section || near) return;
    return watchNear(section, (v) => v && (near = true));
  });

  let hl = $state.raw<Highlighted | null>(null);
  $effect(() => {
    if (!near || collapsed) return;
    const f = file;
    const lines = fileLines;
    let live = true;
    highlightHunks(f.path, f.hunks, lines ? { start: 1, lines } : undefined).then((h) => live && (hl = h));
    return () => (live = false);
  });

  function rowHas(r: Row, side: Side, line: number): boolean {
    if (r.type === 'line') return side === 'old' ? r.kind === 'del' && r.old === line : r.kind !== 'del' && r.new === line;
    if (r.type === 'pair') return side === 'old' ? r.left?.kind === 'del' && r.left.no === line : r.right?.no === line;
    return false;
  }

  function html(side: Side, no: number | undefined, alt?: number): string | undefined {
    if (!hl || no === undefined) return undefined;
    return hl[side].get(no) ?? (alt !== undefined ? hl.new.get(alt) : undefined);
  }

  function down(e: PointerEvent, side: Side, line: number) {
    if (e.button !== 0 || store.readOnly) return;
    e.preventDefault();
    store.draft = D.click(store.draft, file.path, side, line, e.shiftKey);
    const up = () => {
      store.draft = D.finish(store.draft);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointerup', up);
  }

  function tapLine(side: Side, line: number) {
    if (store.readOnly) return;
    store.draft = D.finish(D.click(store.draft, file.path, side, line, false));
  }

  function enter(side: Side, line: number) {
    if (store.draft?.selecting) store.draft = D.extend(store.draft, file.path, side, line);
  }

  function cellSel(c: Cell, right: Cell | undefined): { side: Side; line: number } {
    if (c.kind === 'del') return { side: 'old', line: c.no };
    if (c.kind === 'add') return { side: 'new', line: c.no };
    return { side: 'new', line: right?.no ?? c.no };
  }

  const sub = $derived(file.submodule ?? null);
  const short = (s: string | null | undefined) => (s ? s.slice(0, 7) : '');

  function reviewSub() {
    const wt = store.diff?.worktree;
    if (!wt) return;
    const path = `${wt}/${file.path}`;
    if (sub?.from && sub.to) saveBase(path, sub.from);
    openReviewNear({ worktree: path }, reviewKey(store.target));
  }

  function foldLabel(r: FoldRow): string {
    if (r.kind !== 'move') return t(`review.fold.${r.kind}` as Key);
    const kinds = new Set(file.hunks[r.hunk].lines.filter((l) => l.kind !== 'ctx').map((l) => l.kind));
    if (!r.ref) return t('review.fold.move');
    return t(kinds.size === 1 && kinds.has('add') ? 'review.fold.moveFrom' : kinds.size === 1 ? 'review.fold.moveTo' : 'review.fold.move', { path: r.ref.path });
  }

  const tint = (tag: TagKind | undefined, sig: boolean | undefined) => (sig ? 'var(--accent)' : tag && tag !== 'logic' ? TAG_COLOR[tag] : undefined);

  const covered = (side: Side, line: number) => D.covers(store.draft, file.path, side, line);
  const selLines = $derived(draft ? linesFor(file, draft.side, draft.start, draft.end, fileLines) : []);
</script>

{#snippet gutterBtn(side: Side, line: number)}
  <button class="plus" tabindex="-1" aria-label={t('review.commentLine')} title={t('review.commentLine')} onpointerdown={(e) => down(e, side, line)}><Plus size={11} strokeWidth={2.5} /></button>
{/snippet}

{#snippet code(h: string | undefined, text: string)}
  {#if h !== undefined}<span class="code">{@html h || ' '}</span>{:else}<span class="code">{text || ' '}</span>{/if}
{/snippet}

{#if file.status === 'submodule'}
<section class="file sub" data-path={file.path}>
  <header class="fhead">
    <span class="chev"><Boxes size={14} /></span>
    <span class="st st-submodule">S</span>
    <span class="path mono" title={file.path}>{file.path}</span>
    <span class="range mono faint">
      {#if sub?.from && sub.to}{short(sub.from)} → {short(sub.to)}{:else if sub?.to}{t('review.subAdded', { sha: short(sub.to) })}{:else if sub?.from}{t('review.subRemoved', { sha: short(sub.from) })}{/if}
    </span>
    {#if sub?.to}<button class="btn ghost small" onclick={reviewSub}>{t('review.reviewSubmodule')}</button>{/if}
  </header>
</section>
{:else}
<section class="file" class:ro={store.readOnly} data-path={file.path} bind:this={section}>
  <header class="fhead">
    <button class="chev" class:open={!collapsed} aria-label={t('review.toggleFile')} onclick={() => (store.collapsed[file.path] = !collapsed)}><ChevronRight size={14} /></button>
    <span class="st st-{file.status}">{STATUS[file.status]}</span>
    <span class="path mono" title={file.path}>
      {#if (file.status === 'renamed' || file.status === 'copied') && file.oldPath !== file.path}<span class="faint">{file.oldPath} → </span>{/if}{file.path}
    </span>
    <span class="n-add">+{file.additions}</span>
    <span class="n-del">−{file.deletions}</span>
    {#if !store.readOnly}
      <label class="viewed" class:on={file.viewed}>
        <input type="checkbox" checked={file.viewed} onchange={() => store.toggleViewed(file.path)} />
        {t('review.viewed')}
      </label>
    {/if}
  </header>
  {#if !collapsed}
    {#if placed.top.length}
      <div class="top">
        {#each placed.top as th (th.id)}<Thread {store} thread={th} showCode />{/each}
      </div>
    {/if}
    {#if file.binary}
      <p class="note faint">{t('review.binary')}</p>
    {:else if file.truncated}
      <p class="note faint">{t('review.tooLarge')}</p>
    {:else}
      <div class="grid mono" class:split role="grid" tabindex="-1">
        {#each chunked as ch, ci (ci)}
          <Lazy height={ch.height} pinned={pinned.has(ci)}>
            {#each rows.slice(ch.start, ch.end) as r, j (j)}
              {@const i = ch.start + j}
              {#if r.type === 'gap'}
                {#if r.count !== 0 || r.header}
                  <div class="gap" data-hunk={r.id === 'end' ? undefined : parseInt(r.id.slice(1))}>
                    {#if r.count !== 0}
                      <button class="expand" onclick={() => store.expand(file.path, r.id)} title={r.count ? t('review.expand', { n: r.count }) : t('review.expandRest')}>
                        <UnfoldVertical size={13} />{r.count ? t('review.expand', { n: r.count }) : t('review.expandRest')}
                      </button>
                    {/if}
                    <span class="hh">{r.header}</span>
                  </div>
                {/if}
              {:else if r.type === 'fold'}
                <button class="fold" data-hunk={r.hunk} style:--tag={TAG_COLOR[r.kind === 'mixed' ? 'format' : r.kind]} onclick={() => store.unfold(file.path, r.hunk)}>
                  <span class="fl">{foldLabel(r)}</span>
                  <span class="faint">{t('review.fold.lines', { n: r.count })}</span>
                  <span class="show">{t('review.fold.show')}<ChevronRight size={12} /></span>
                </button>
              {:else if r.type === 'line'}
                {@const side = (r.kind === 'del' ? 'old' : 'new') as Side}
                {@const no = side === 'old' ? r.old! : r.new!}
                <div class="ln {r.kind}" class:sel={covered(side, no)} class:tagged={!!tint(r.tag, r.sig)} style:--tag={tint(r.tag, r.sig)} role="row" tabindex="-1" onpointerenter={() => enter(side, no)}>
                  {#if phone}
                    <button class="nos" aria-label={t('mobile.review.commentOn', { path: file.path, line: no })} onclick={() => tapLine(side, no)}>
                      <span class="no">{r.old ?? ''}</span><span class="no">{r.new ?? ''}</span>
                    </button>
                  {:else}
                    <span class="no">{r.old ?? ''}</span>
                    <span class="no">{r.new ?? ''}</span>
                    {@render gutterBtn(side, no)}
                  {/if}
                  <span class="sign">{r.kind === 'add' ? '+' : r.kind === 'del' ? '-' : ' '}</span>
                  {@render code(html(side, no, r.kind === 'ctx' ? r.new : undefined), r.text)}
                </div>
              {:else}
                <div class="pair" role="row">
                  {#each [r.left, r.right] as c, k (k)}
                    {#if c}
                      {@const s = cellSel(c, r.right)}
                      <div class="half {c.kind}" class:sel={covered(s.side, s.line)} class:tagged={!!tint(c.tag, c.sig)} style:--tag={tint(c.tag, c.sig)} role="gridcell" tabindex="-1" onpointerenter={() => enter(s.side, s.line)}>
                        <span class="no">{c.no}</span>
                        {@render gutterBtn(s.side, s.line)}
                        <span class="sign">{c.kind === 'add' ? '+' : c.kind === 'del' ? '-' : ' '}</span>
                        {@render code(html(k === 0 ? 'old' : 'new', c.no, c.kind === 'ctx' ? r.right?.no : undefined), c.text)}
                      </div>
                    {:else}
                      <div class="half empty"></div>
                    {/if}
                  {/each}
                </div>
              {/if}
              {#if placed.byRow.get(i)}
                <div class="threads">
                  {#each placed.byRow.get(i) ?? [] as th (th.id)}<Thread {store} thread={th} />{/each}
                </div>
              {/if}
              {#if i === draftRow && store.draft && !phone}
                <div class="threads">
                  <div class="draft">
                    <div class="dlabel faint">{draft!.start === draft!.end ? `L${draft!.start}` : `L${draft!.start}–${draft!.end}`}{draft!.side === 'old' ? ' (base)' : ''}</div>
                    <CommentBox bind:value={() => store.draft?.body ?? '', (v) => store.draft && (store.draft = D.setBody(store.draft, v))} label={t('review.addComment')} lines={selLines} onsubmit={() => store.addDraft()} oncancel={() => (store.draft = null)} />
                  </div>
                </div>
              {/if}
            {/each}
          </Lazy>
        {/each}
      </div>
    {/if}
  {/if}
</section>
{/if}

{#if phone && draft && !draft.selecting}
  <Modal title={t('mobile.review.commentOn', { path: file.path.split('/').pop() ?? file.path, line: draft.start })} onclose={() => (store.draft = null)} width={560}>
    <div class="sheet">
      {#if selLines.length}<pre class="ctx mono">{selLines.join('\n')}</pre>{/if}
      <CommentBox bind:value={() => store.draft?.body ?? '', (v) => store.draft && (store.draft = D.setBody(store.draft, v))} label={t('review.addComment')} lines={selLines} rows={4} onsubmit={() => store.addDraft()} oncancel={() => (store.draft = null)} />
    </div>
  </Modal>
{/if}

<style>
  .file {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    margin-bottom: 14px;
  }
  .fhead {
    position: sticky;
    top: -14px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 10px 0 6px;
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    font-size: 12px;
  }
  .chev {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    color: var(--text-2);
    transition: transform var(--ease);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .chev:hover {
    background: var(--surface-hover);
  }
  .st {
    font-family: var(--mono);
    font-size: 10.5px;
    font-weight: 600;
    width: 16px;
    text-align: center;
  }
  .st-added,
  .st-untracked {
    color: var(--diff-add);
  }
  .st-deleted {
    color: var(--diff-del);
  }
  .st-modified {
    color: #b45309;
  }
  :global([data-theme='dark']) .st-modified {
    color: #fbbf24;
  }
  .st-renamed,
  .st-submodule {
    color: var(--diff-hunk);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 500;
  }
  .sub .fhead {
    flex-wrap: wrap;
    height: auto;
    min-height: 38px;
    row-gap: 4px;
    padding-top: 6px;
    padding-bottom: 6px;
    border-bottom: 0;
    border-radius: var(--radius-lg);
  }
  .sub .path {
    flex: 1 1 120px;
  }
  .range {
    flex: none;
    font-size: 11.5px;
  }
  .n-add {
    color: var(--diff-add);
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .n-del {
    color: var(--diff-del);
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .ro :global(.plus) {
    display: none;
  }
  .viewed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 8px;
    margin-left: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-2);
    cursor: pointer;
    font-size: 11.5px;
    background: var(--surface);
  }
  .viewed.on {
    color: var(--text);
    background: var(--surface-2);
  }
  .viewed input {
    margin: 0;
    accent-color: var(--accent);
  }
  .note {
    margin: 0;
    padding: 14px 16px;
    font-size: 12px;
  }
  .top {
    padding-top: 4px;
    border-bottom: 1px solid var(--border);
  }
  .grid {
    font-size: 12px;
    line-height: 20px;
    overflow-x: auto;
    user-select: text;
    -webkit-user-select: text;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
  }
  .gap {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 28px;
    padding: 0 10px;
    background: color-mix(in srgb, var(--diff-hunk) 6%, var(--bg-subtle));
    color: var(--text-3);
    white-space: pre;
    min-width: max-content;
  }
  .expand {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 8px;
    border-radius: 4px;
    color: var(--diff-hunk);
    font-family: var(--font);
    font-size: 11.5px;
  }
  .expand:hover {
    background: var(--surface-hover);
  }
  .hh {
    font-size: 11.5px;
  }
  .fold {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 28px;
    padding: 0 10px 0 13px;
    border-radius: 0;
    box-shadow: inset 3px 0 0 var(--tag);
    background: color-mix(in srgb, var(--tag) 7%, var(--bg-subtle));
    color: var(--text-2);
    font-family: var(--font);
    font-size: 11.5px;
    text-align: left;
  }
  .fold:hover {
    background: color-mix(in srgb, var(--tag) 13%, var(--bg-subtle));
  }
  .fold .fl {
    color: var(--text);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fold .show {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    color: var(--diff-hunk);
  }
  .ln.tagged,
  .half.tagged {
    box-shadow: inset 3px 0 0 var(--tag);
  }
  .ln,
  .half {
    display: flex;
    position: relative;
    white-space: pre;
    min-width: max-content;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .half {
    min-width: 0;
    overflow: hidden;
  }
  .half + .half,
  .half.empty + .half {
    border-left: 1px solid var(--border);
  }
  .half.empty {
    background: var(--bg-subtle);
  }
  .ln.add,
  .half.add {
    background: var(--diff-add-bg);
  }
  .ln.del,
  .half.del {
    background: var(--diff-del-bg);
  }
  .sel,
  .ln.sel,
  .half.sel {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .no {
    width: 46px;
    flex: none;
    padding-right: 8px;
    text-align: right;
    color: var(--text-3);
    user-select: none;
    -webkit-user-select: none;
    font-size: 11px;
  }
  .split .no {
    width: 48px;
  }
  .sign {
    width: 16px;
    flex: none;
    color: var(--text-3);
    user-select: none;
    -webkit-user-select: none;
    text-align: center;
  }
  .add .sign {
    color: var(--diff-add);
  }
  .del .sign {
    color: var(--diff-del);
  }
  .code {
    padding-right: 16px;
  }
  .split .code {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .plus {
    position: absolute;
    left: 92px;
    top: 2px;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 4px;
    background: var(--accent);
    color: var(--accent-fg);
    opacity: 0;
    transform: scale(0.85);
    transition:
      opacity var(--ease),
      transform var(--ease);
    cursor: pointer;
  }
  .split .plus {
    left: 48px;
  }
  .ln:hover .plus,
  .half:hover .plus,
  .sel .plus {
    opacity: 1;
    transform: none;
  }
  .threads {
    padding: 2px 0;
    background: var(--bg-subtle);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    white-space: normal;
  }
  .draft {
    margin: 6px 12px 8px;
    padding: 10px 12px;
    max-width: 860px;
    border: 1px solid var(--accent);
    border-radius: var(--radius-lg);
    background: var(--surface);
    font-family: var(--font);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .dlabel {
    font-family: var(--mono);
    font-size: 11px;
    margin-bottom: 6px;
  }
  .nos {
    display: flex;
    flex: none;
    align-items: stretch;
    padding: 0;
    border-radius: 0;
    font: inherit;
    color: inherit;
    background: transparent;
  }
  .nos:active .no {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .nos .no {
    width: 38px;
  }
  .sheet {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-bottom: env(safe-area-inset-bottom);
  }
  .sheet .ctx {
    margin: 0;
    max-height: 7.5em;
    overflow: auto;
    padding: 8px 10px;
    font-size: 12px;
    line-height: 1.5;
    white-space: pre;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-subtle);
    color: var(--text-2);
  }
  :global(.phone) .grid {
    line-height: 24px;
  }
  :global(.phone) .fhead {
    top: 0;
    height: 48px;
    border-radius: 0;
  }
  :global(.phone) .sub .fhead {
    position: static;
    height: auto;
    min-height: 48px;
  }
  :global(.phone) .file {
    border-radius: 0;
    border-left: none;
    border-right: none;
  }
  :global(.phone) .chev {
    width: 40px;
    height: 40px;
  }
  :global(.phone) .viewed {
    height: 36px;
  }
  :global(.phone) .expand,
  :global(.phone) .fold {
    min-height: 36px;
  }
  .code :global(.hljs-keyword),
  .code :global(.hljs-built_in),
  .code :global(.hljs-literal),
  .code :global(.hljs-selector-tag),
  .code :global(.hljs-meta .hljs-keyword) {
    color: var(--hl-keyword);
  }
  .code :global(.hljs-string),
  .code :global(.hljs-regexp),
  .code :global(.hljs-attr),
  .code :global(.hljs-symbol),
  .code :global(.hljs-template-tag) {
    color: var(--hl-string);
  }
  .code :global(.hljs-number),
  .code :global(.hljs-variable.constant_) {
    color: var(--hl-number);
  }
  .code :global(.hljs-comment),
  .code :global(.hljs-quote) {
    color: var(--hl-comment);
    font-style: italic;
  }
  .code :global(.hljs-title),
  .code :global(.hljs-section) {
    color: var(--hl-title);
  }
  .code :global(.hljs-type),
  .code :global(.hljs-class),
  .code :global(.hljs-name),
  .code :global(.hljs-tag),
  .code :global(.hljs-selector-class) {
    color: var(--hl-type);
  }
  .code :global(.hljs-meta),
  .code :global(.hljs-attribute),
  .code :global(.hljs-params) {
    color: var(--hl-meta);
  }
  .grid {
    --hl-keyword: #a626a4;
    --hl-string: #2f7a3a;
    --hl-number: #b45309;
    --hl-comment: #8b8b94;
    --hl-title: #2557c7;
    --hl-type: #0e7490;
    --hl-meta: #6b6b76;
  }
  :global([data-theme='dark']) .grid {
    --hl-keyword: #d19bf0;
    --hl-string: #9fd59a;
    --hl-number: #f0b471;
    --hl-comment: #6e6e78;
    --hl-title: #8fb4ff;
    --hl-type: #73cfe0;
    --hl-meta: #a1a1aa;
  }
</style>
