<script lang="ts" module>
  import { routeTermUrls } from '../../termlinks';
  import { app as shared } from '../../state.svelte';

  if (typeof window !== 'undefined') routeTermUrls(window, (url, termId, shift) => (shared.openUrl(url, { termId }, { shift, inTerminal: true }), true));
</script>

<script lang="ts">
  import { dropZone } from '../../drop';
  import { onMount, untrack } from 'svelte';
  import type { Terminal } from 'ghostty-web';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import Copy from '@lucide/svelte/icons/copy';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import X from '@lucide/svelte/icons/x';
  import { findHit, viewportFor, type Hit } from '../../termfind';
  import { isTauri, type TermIO } from '../../client';
  import { t } from '../../i18n.svelte';
  import { attachIme } from '../../ime';
  import { keyToInput } from '../../keys';
  import { cellAt, clickMove, ModeTracker, WheelAccumulator, wheelAction } from '../../mouse';
  import { resetViewport, scrollbackBytes, writeAnchored } from '../../scroll';
  import { fontChain, fontFamilies, lineMetrics, sameMetrics, SYMBOL_SAMPLE } from '../../fonts';
  import { applyMods, NO_MODS, type Mods } from '../../mobile/accessory';
  import { attachRetryMs } from '../../conn';
  import { termLink, type TermLink } from '../../termlink';
  import { LONG_PRESS_MS, movedTooFar, pinchFontSize, touchDistance } from '../../mobile/gestures';
  import { canResize, fitTo, isAgentTerm, shouldReassert, sizedElsewhere } from '../../mobile/termsize';
  import { ui } from '../../mobile/ui.svelte';
  import { isNativeAndroid, native } from '../../mobile/native';
  import { colorPalette, customColors, resolvedTheme, saveSettings, settings } from '../../settings.svelte';
  import { app } from '../../state.svelte';
  import { loadGhostty, termTheme } from '../../terminal';
  import { pathRanges } from '../../termlinks';
  import { scheduler, type PaintTarget } from '../../termrender';
  import { routeCopies } from '../../selection-copy';
  import { guardAutoScroll } from '../../selection-scroll';
  import { settle } from '../../settle';
  import { openFile } from '../../preview';
  import type { UploadResult } from '../../types';
  import { basename, bracketed, insertText, planDrop, planPaste, PROGRESS_FROM, uploadedPath, type Refusal } from '../../uploads';
  const accessoryBar = () => import('../mobile/AccessoryBar.svelte');

  let { termId, focused }: { termId: string; focused: boolean } = $props();

  let el: HTMLDivElement;
  let pane: HTMLDivElement;
  let dropping = $state(false);
  let picker = $state<HTMLInputElement>();
  let term: Terminal | null = null;
  let io: TermIO | null = null;
  let link: TermLink | null = null;
  let refit: (() => void) | null = null;
  let pickFiles: ((files: File[]) => void) | null = null;
  let pastePath: ((path: string) => Promise<void>) | null = null;

  interface Metrics {
    width: number;
    height: number;
    baseline: number;
  }
  interface Internals {
    renderer?: { remeasureFont(): void; metrics: Metrics };
    handleFontChange?: () => void;
  }

  async function loadFonts(custom: string, size: number) {
    const fonts = document.fonts;
    if (!fonts) return;
    await Promise.all(fontFamilies(custom).map((f) => fonts.load(`${size}px "${f}"`, `M${SYMBOL_SAMPLE}`).catch(() => [])));
  }

  function applyFont() {
    const tm = term;
    if (!tm) return;
    const family = fontChain(settings.terminalFont);
    if (tm.options.fontSize !== fontSize) tm.options.fontSize = fontSize;
    if (tm.options.fontFamily !== family) tm.options.fontFamily = family;
    const internals = tm as unknown as Internals;
    const r = internals.renderer;
    if (r && typeof r.remeasureFont === 'function') {
      const before = r.metrics;
      r.remeasureFont();
      r.metrics = lineMetrics(r.metrics, settings.lineHeight);
      if (!sameMetrics(before, r.metrics)) internals.handleFontChange?.();
    }
    refit?.();
  }
  let loading = $state(true);
  let closed = $state(false);
  let error = $state<string | null>(null);

  const info = $derived(app.data.terms.find((x) => x.id === termId));
  const exited = $derived(closed || !!info?.exited);
  const remote = $derived(app.conn.target.kind !== 'local');
  const message = (e: unknown) => String((e as Error)?.message ?? e);
  let mine = $state<{ cols: number; rows: number } | null>(null);
  let mods = $state<Mods>({ ...NO_MODS });
  let composing = $state(false);
  const composeSheet = () => import('../mobile/ComposeSheet.svelte');
  let copyAt = $state<{ x: number; y: number } | null>(null);
  let shownHint = $state<ReturnType<typeof sizedElsewhere>>(null);
  let scrolledBack = $state(false);
  let unseen = $state(false);
  let writing = false;

  const phone = $derived(ui.layout === 'phone');
  const fixed = $derived(phone && !!info && !canResize('phone', isAgentTerm(info)));
  const hint = $derived(sizedElsewhere(info, mine, phone ? 'phone' : 'desktop'));
  const fontSize = $derived(phone ? (settings.phoneFontSize ?? 12) : settings.fontSize);
  const composeKey = $derived(info?.lineId ? `line:${info.lineId}` : `term:${termId}`);

  function measured() {
    if (term) mine = { cols: term.cols, rows: term.rows };
  }

  function reassert() {
    if (!term || !io || fixed) return;
    refit?.();
    io.resize(term.cols, term.rows);
    measured();
  }

  function visible(): boolean {
    return typeof document === 'undefined' || document.visibilityState === 'visible';
  }

  function maybeReassert() {
    if (shouldReassert(visible(), focused, hint, !fixed)) reassert();
  }

  let focusFrame = 0;
  function focusSoon() {
    cancelAnimationFrame(focusFrame);
    focusFrame = requestAnimationFrame(() => {
      if (term && el.isConnected && document.activeElement !== el) el.focus({ preventScroll: true });
    });
  }

  function toBottom() {
    term?.scrollToBottom();
  }

  function write(data: string) {
    toBottom();
    io?.write(data);
  }

  onMount(() => {
    let ro: ResizeObserver | null = null;
    const resized = settle(() => refit?.(), 100);
    let stopIme: (() => void) | null = null;
    let stopShown: (() => void) | null = null;
    let disposed = false;
    const onVisibility = () => {
      if (visible()) maybeReassert();
    };
    window.addEventListener('focus', maybeReassert);
    document.addEventListener('visibilitychange', onVisibility);
    const stopTouch = touchGestures();
    const modes = new ModeTracker();
    const wheel = new WheelAccumulator();

    const send = (text: string) => {
      if (text) {
        toBottom();
        io?.write(bracketed(text, term?.hasBracketedPaste() ?? false));
      }
      term?.focus();
    };
    const refuse = (r: Refusal) => app.notify(t(r.reason === 'folder' ? 'upload.folder' : 'upload.tooLarge', { name: r.name }), 'error');

    const uploadAll = async (paths: string[]): Promise<string[]> => {
      const done: UploadResult[] = [];
      let failure: string | null = null;
      for (const p of paths) {
        const name = basename(p);
        try {
          done.push(await app.client!.uploadFile(p, ({ sent, total }) => total > PROGRESS_FROM && app.progress(t('upload.progress', { name }), sent / total)));
        } catch (e) {
          failure = t('upload.failed', { name, error: message(e) });
        }
      }
      if (failure) app.notify(failure, 'error', undefined, 'upload');
      else if (done.length) {
        const text = done.length === 1 ? t('upload.done', { name: done[0].name, path: done[0].short ?? done[0].path }) : t('upload.doneMany', { n: done.length });
        app.notice({ kind: 'upload', title: text, quiet: true, toast: false });
        app.notify(text);
      }
      return done.map(uploadedPath);
    };

    const pasteFiles = async (files: File[]) => {
      const saved: string[] = [];
      for (const f of files) {
        const name = f.name || 'paste.png';
        try {
          saved.push(await app.client!.savePaste(name, new Uint8Array(await f.arrayBuffer())));
        } catch (e) {
          return app.notify(t('upload.pasteFailed', { name, error: message(e) }), 'error');
        }
      }
      send(insertText(await uploadAll(saved)));
    };

    pastePath = async (path: string) => send(insertText(remote ? await uploadAll([path]) : [path]));

    const onPaste = (e: ClipboardEvent) => {
      const dt = e.clipboardData;
      if (!dt || !io) return;
      const files = Array.from(dt.files);
      if (!files.length)
        for (const item of Array.from(dt.items)) {
          const f = item.kind === 'file' ? item.getAsFile() : null;
          if (f) files.push(f);
        }
      if (!files.length && !dt.getData('text/plain') && isNativeAndroid()) {
        e.preventDefault();
        e.stopImmediatePropagation();
        void pasteImage();
        return;
      }
      const plan = planPaste({ text: dt.getData('text/plain'), files: files.map((f) => ({ name: f.name, type: f.type, size: f.size })) }, remote);
      if (plan.kind === 'native') return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (plan.kind === 'reject') return refuse(plan);
      void pasteFiles(files);
    };
    pane.addEventListener('paste', onPaste, true);
    const onClick = (e: MouseEvent) => {
      const tm = term;
      if (!tm || !io || phone || e.button !== 0 || e.detail !== 1 || e.shiftKey || e.altKey || e.ctrlKey || e.metaKey) return;
      if (tm.hasSelection() || tm.getViewportY() > 0) return;
      const rect = (el.querySelector('canvas') ?? el).getBoundingClientRect();
      const { col, row } = cellAt(e.clientX, e.clientY, rect, tm.cols, tm.rows);
      const buf = tm.buffer.active;
      const line = buf.getLine(buf.length - tm.rows + buf.cursorY);
      const keys = clickMove(col - 1, row - 1, { x: buf.cursorX, y: buf.cursorY }, (x) => line?.getCell(x)?.getWidth() ?? 1, modes.modes());
      if (keys) io.write(keys);
    };
    el.addEventListener('click', onClick);

    pickFiles = (files: File[]) => {
      if (!files.length) return;
      const plan = planPaste({ text: '', files: files.map((f) => ({ name: f.name, type: f.type, size: f.size })) }, remote);
      if (plan.kind === 'reject') return refuse(plan);
      if (plan.kind === 'save') void pasteFiles(files);
    };

    const onDrop = async (paths: string[]) => {
      const plan = planDrop({ paths }, remote);
      if (plan.kind === 'none') return;
      if (plan.kind === 'reject') return refuse(plan);
      send(insertText(plan.kind === 'type' ? plan.paths : await uploadAll(plan.paths)));
    };
    const offDrop = dropZone({
      el: () => (io ? pane : null),
      rank: 1,
      hover: (on) => (dropping = on),
      drop: (paths) => void onDrop(paths),
    });

    (async () => {
      try {
        const { mod: g, ghostty } = await loadGhostty();
        await loadFonts(settings.terminalFont, fontSize);
        if (disposed) return;
        const theme = resolvedTheme();
        term = new g.Terminal({
          ghostty,
          fontFamily: fontChain(settings.terminalFont),
          fontSize,
          theme: termTheme(theme, customColors(), colorPalette()),
          cursorBlink: true,
          scrollback: scrollbackBytes(settings.scrollbackLines),
        });
        const fit = new g.FitAddon();
        term.loadAddon(fit);
        term.open(el);
        const tl = term;
        const sm = (tl as unknown as { selectionManager?: Parameters<typeof guardAutoScroll>[0] & Parameters<typeof routeCopies>[0] }).selectionManager;
        guardAutoScroll(sm);
        routeCopies(sm, (text) => app.client?.copyText(text) ?? Promise.resolve());
        tl.registerLinkProvider({
          provideLinks(y, cb) {
            const line = tl.buffer.active.getLine(y);
            const links = line
              ? pathRanges(line).map(({ path, x0, x1 }) => ({
                  text: path,
                  range: { start: { x: x0, y }, end: { x: x1, y } },
                  activate: () => {
                    if (!tl.hasSelection()) openFile(path, termId);
                  },
                }))
              : [];
            cb(links.length ? links : undefined);
          },
        });
        stopShown = scheduler.watch(term as unknown as PaintTarget, (shown) => io?.background?.(!shown));
        quietInput(el);
        const area = el.querySelector('textarea');
        if (area) quietInput(area);
        refit = () => {
          const tm = term;
          if (!tm) return;
          if (!fixed) return void fitTo(tm, fit.proposeDimensions());
          if (info && (tm.cols !== info.cols || tm.rows !== info.rows)) tm.resize(info.cols, info.rows);
        };
        applyFont();
        link = termLink({
          open: (onData, onClose) => app.client!.attachTerm(termId, onData, onClose),
          reset: () => {
            if (term) resetViewport(term);
            scrolledBack = false;
            unseen = false;
            modes.reset();
          },
          data: (bytes) => {
            modes.feed(bytes);
            if (!term) return;
            writing = true;
            try {
              scrolledBack = writeAnchored(term, bytes);
              if (scrolledBack) unseen = true;
            } finally {
              writing = false;
            }
          },
          up: (conn) => {
            io = conn;
            error = null;
            if (!term) return;
            if (!scheduler.shown(term as unknown as PaintTarget)) conn.background?.(true);
            if (!fixed) conn.resize(term.cols, term.rows);
            measured();
          },
          down: () => (io = null),
          failed: (e) => (error = message(e)),
          retryMs: (attempt, e) => attachRetryMs(attempt, !!info?.exited, e),
          ended: () => (closed = true),
        });
        term.onScroll(() => {
          if (writing || !term) return;
          scrolledBack = term.getViewportY() > 0;
          if (!scrolledBack) unseen = false;
        });
        const typed = (d: string) => {
          if (!writing) toBottom();
          if (mods.ctrl === 'off' && mods.alt === 'off') return void io?.write(d);
          const p = applyMods(d, mods);
          mods = p.mods;
          io?.write(p.send ?? d);
        };
        term.onData(typed);
        stopIme = attachIme(el, typed);
        term.attachCustomKeyEventHandler((e) => {
          const conn = io;
          if (e.type !== 'keydown' || !conn) return false;
          const input = keyToInput(e, { shiftEnterNewline: settings.shiftEnterNewline });
          if (input === null) return false;
          toBottom();
          conn.write(input);
          return true;
        });
        term.attachCustomWheelEventHandler((e) => {
          const m = modes.modes();
          const tm = term;
          if (!tm || phone || (!m.tracking && !m.alt) || (e.shiftKey && !m.alt)) return false;
          const canvas = el.querySelector('canvas');
          const rect = (canvas ?? el).getBoundingClientRect();
          const steps = wheel.steps(e.deltaY, e.deltaMode as 0 | 1 | 2, { cellHeight: rect.height / Math.max(1, tm.rows), rows: tm.rows });
          const cell = cellAt(e.clientX, e.clientY, rect, tm.cols, tm.rows);
          const action = wheelAction(steps, { deltaY: e.deltaY, deltaMode: e.deltaMode as 0 | 1 | 2, ...cell, shift: e.shiftKey, alt: e.altKey, ctrl: e.ctrlKey }, m);
          if (action.kind === 'send') io?.write(action.data);
          else if (action.kind === 'local') tm.scrollLines(action.lines);
          return true;
        });
        term.onResize(({ cols, rows }) => {
          mine = { cols, rows };
          link?.reattach();
        });
        await link.attach();
        ro = new ResizeObserver(([e]) => {
          if (!phone) refit?.();
          resized.push(`${Math.round(e.contentRect.width)}x${Math.round(e.contentRect.height)}`);
        });
        ro.observe(el);
        if (focused && (!phone || ui.tab === 'agents')) focusSoon();
      } catch (e) {
        error = String((e as Error)?.message ?? e);
      } finally {
        loading = false;
      }
    })();

    const onFontsLoaded = () => applyFont();
    document.fonts?.addEventListener('loadingdone', onFontsLoaded);

    return () => {
      disposed = true;
      cancelAnimationFrame(focusFrame);
      window.removeEventListener('focus', maybeReassert);
      document.removeEventListener('visibilitychange', onVisibility);
      stopTouch();
      pickFiles = null;
      pastePath = null;
      document.fonts?.removeEventListener('loadingdone', onFontsLoaded);
      refit = null;
      pane.removeEventListener('paste', onPaste, true);
      el.removeEventListener('click', onClick);
      offDrop();
      ro?.disconnect();
      resized.cancel();
      stopIme?.();
      stopShown?.();
      link?.dispose();
      link = null;
      io = null;
      term?.dispose();
      term = null;
    };
  });

  function quietInput(node: HTMLElement) {
    node.setAttribute('autocomplete', 'off');
    node.setAttribute('autocorrect', 'off');
    node.setAttribute('autocapitalize', 'off');
    node.setAttribute('spellcheck', 'false');
    node.setAttribute('writingsuggestions', 'false');
  }

  function touchGestures(): () => void {
    let pinch: { distance: number; size: number; live: number } | null = null;
    let press: { x: number; y: number; timer: ReturnType<typeof setTimeout> } | null = null;
    const cancelPress = () => {
      if (press) clearTimeout(press.timer);
      press = null;
    };
    const start = (e: TouchEvent) => {
      if (!phone) return;
      cancelPress();
      if (e.touches.length === 2) {
        pinch = { distance: touchDistance(e.touches[0], e.touches[1]), size: fontSize, live: fontSize };
        return;
      }
      if (e.touches.length !== 1) return;
      const tch = e.touches[0];
      const box = el.getBoundingClientRect();
      const at = { x: tch.clientX - box.left, y: tch.clientY - box.top };
      copyAt = null;
      const timer = setTimeout(() => {
        copyAt = at;
        press = null;
      }, LONG_PRESS_MS);
      press = { x: tch.clientX, y: tch.clientY, timer };
    };
    const move = (e: TouchEvent) => {
      if (pinch && e.touches.length === 2) {
        e.preventDefault();
        pinch.live = pinchFontSize(pinch.size, pinch.distance, touchDistance(e.touches[0], e.touches[1]));
        if (term && term.options.fontSize !== pinch.live) term.options.fontSize = pinch.live;
      } else if (press && e.touches.length === 1 && movedTooFar(e.touches[0].clientX - press.x, e.touches[0].clientY - press.y)) {
        cancelPress();
      }
    };
    const end = (e: TouchEvent) => {
      cancelPress();
      if (pinch && e.touches.length < 2) {
        const size = pinch.live;
        pinch = null;
        if (size !== settings.phoneFontSize) saveSettings({ phoneFontSize: size });
        refit?.();
      }
    };
    el.addEventListener('touchstart', start, { passive: true });
    el.addEventListener('touchmove', move, { passive: false });
    el.addEventListener('touchend', end, { passive: true });
    el.addEventListener('touchcancel', end, { passive: true });
    return () => {
      cancelPress();
      el.removeEventListener('touchstart', start);
      el.removeEventListener('touchmove', move);
      el.removeEventListener('touchend', end);
      el.removeEventListener('touchcancel', end);
    };
  }

  function screenText(tm: Terminal): string {
    const b = tm.buffer.active;
    const lines: string[] = [];
    for (let y = Math.max(0, b.length - tm.rows); y < b.length; y++) lines.push(b.getLine(y)?.translateToString(true) ?? '');
    return lines.join('\n').replace(/\s+$/, '');
  }

  async function copy() {
    copyAt = null;
    if (!term) return;
    const text = term.hasSelection() ? term.getSelection() : screenText(term);
    try {
      await app.client?.copyText(text);
      app.notify(t('mobile.term.copied'));
    } catch (e) {
      app.notify(String((e as Error)?.message ?? e), 'error');
    }
  }

  async function pasteImage(): Promise<boolean> {
    try {
      const path = await native.clipboardImage();
      if (!path || !pastePath) return false;
      await pastePath(path);
    } catch (e) {
      app.notify(t('upload.pasteFailed', { name: t('mobile.term.clipboard'), error: String((e as Error)?.message ?? e) }), 'error');
    }
    return true;
  }

  async function paste() {
    if (await pasteImage()) return;
    try {
      const text = (await app.client?.pasteText()) ?? '';
      if (!text) return app.notify(t('mobile.term.pasteFailed'));
      write(term?.hasBracketedPaste() ? `\x1b[200~${text}\x1b[201~` : text);
    } catch {
      app.notify(t('mobile.term.pasteFailed'));
    }
  }

  let finding = $state(false);
  let findQuery = $state('');
  let findMiss = $state(false);
  let hit: Hit | null = null;

  function findNext(backwards: boolean) {
    const tm = term;
    if (!tm || !findQuery) return;
    const b = tm.buffer.active;
    const lines: string[] = [];
    for (let y = 0; y < b.length; y++) lines.push(b.getLine(y)?.translateToString(true) ?? '');
    const h = findHit(lines, findQuery, hit, backwards);
    hit = h;
    findMiss = !h;
    if (!h) return void tm.clearSelection();
    const v = viewportFor(h.row, Math.max(0, b.length - tm.rows), tm.rows);
    tm.scrollToLine(v.offset);
    tm.select(h.col, v.row, findQuery.length);
  }

  function closeFind() {
    finding = false;
    hit = null;
    findMiss = false;
    term?.clearSelection();
    term?.focus();
  }

  function onFindKey(e: KeyboardEvent) {
    if (e.key === 'Enter') findNext(!e.shiftKey);
    else if (e.key === 'Escape') closeFind();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  $effect(() => {
    if (app.conn.phase === 'connected') untrack(() => link?.resume());
  });

  $effect(() => {
    if (!focused || phone) return;
    const onAction = (e: Event) => {
      const action = (e as CustomEvent).detail;
      const tm = term;
      if (action === 'find') {
        e.preventDefault();
        finding = true;
        queueMicrotask(() => pane.querySelector<HTMLInputElement>('.find input')?.select());
      } else if (action === 'paste') {
        e.preventDefault();
        void paste();
      } else if (action === 'copy' && tm?.hasSelection()) {
        e.preventDefault();
        app.client?.copyText(tm.getSelection()).catch(() => {});
      }
    };
    window.addEventListener('sb:term', onAction);
    return () => window.removeEventListener('sb:term', onAction);
  });

  function picked(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    input.value = '';
    pickFiles?.(files);
  }

  $effect(() => {
    const key = `${settings.terminalFont}|${fontSize}|${settings.lineHeight}`;
    if (!term || !key) return;
    loadFonts(settings.terminalFont, untrack(() => fontSize)).then(applyFont);
  });

  $effect(() => {
    void settings.gpuTerminal;
    if (term) scheduler.invalidate(term as unknown as PaintTarget);
  });

  $effect(() => {
    if (focused && app.view !== 'kanban' && (!phone || ui.tab === 'agents')) focusSoon();
  });

  $effect(() => {
    if (!focused) return;
    const frame = requestAnimationFrame(() => maybeReassert());
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    const size = fixed && info ? `${info.cols}x${info.rows}` : '';
    if (size) untrack(() => refit?.());
  });

  $effect(() => {
    const h = hint;
    if (!h) {
      shownHint = null;
      return;
    }
    const timer = setTimeout(() => (shownHint = h), 600);
    return () => clearTimeout(timer);
  });
</script>

<div class="pane" class:phone class:fixed bind:this={pane}>
  <div class="term" data-terminal data-term-id={termId} bind:this={el}></div>
  {#if finding}
    <div class="find" class:miss={findMiss} role="search">
      <input bind:value={findQuery} oninput={() => ((hit = null), (findMiss = false))} onkeydown={onFindKey} placeholder={t('term.find')} aria-label={t('term.find')} spellcheck="false" />
      <button class="icon-btn" aria-label={t('term.findPrev')} title={t('term.findPrev')} onclick={() => findNext(true)}><ChevronUp size={14} /></button>
      <button class="icon-btn" aria-label={t('term.findNext')} title={t('term.findNext')} onclick={() => findNext(false)}><ChevronDown size={14} /></button>
      <button class="icon-btn" aria-label={t('common.close')} title={t('common.close')} onclick={closeFind}><X size={14} /></button>
    </div>
  {/if}
  {#if dropping}
    <div class="drop"><span>{remote ? t('term.dropUpload') : t('term.dropInsert')}</span></div>
  {/if}
  {#if loading}<div class="note faint">{t('term.attaching')}</div>{/if}
  {#if error}<div class="note error">{error}</div>{/if}
  {#if shownHint && !exited}
    <button class="sized" onclick={reassert}>
      {t(shownHint === 'phone' ? 'mobile.term.resizedForPhone' : shownHint === 'desktop' ? 'mobile.term.sizedForDesktop' : 'mobile.term.sizedElsewhere')}
      · <span class="fit">{t('mobile.term.fit')}</span>
    </button>
  {/if}
  {#if copyAt}
    <button class="copy" style:left="{copyAt.x}px" style:top="{copyAt.y}px" onpointerdown={(e) => e.preventDefault()} onclick={copy}>
      <Copy size={15} />{t('mobile.term.copy')}
    </button>
  {/if}
  {#if scrolledBack && unseen && !exited}
    <button class="latest" onclick={() => (toBottom(), term?.focus())}><ArrowDown size={13} />{t('term.newOutput')}</button>
  {/if}
  {#if exited}
    <div class="exited">{t('term.exited', { code: info?.exitCode ?? '–' })}</div>
  {/if}
  {#if phone && !exited}
    {#await accessoryBar() then { default: AccessoryBar }}<AccessoryBar bind:mods onsend={write} onpaste={paste} onupload={() => picker?.click()} oncompose={() => (composing = true)} />{/await}
    <input class="picker" type="file" multiple bind:this={picker} onchange={picked} />
  {/if}
</div>
{#if composing}
  {#await composeSheet() then { default: ComposeSheet }}<ComposeSheet targetKey={composeKey} onclose={() => (composing = false)} />{/await}
{/if}

<style>
  .find {
    position: absolute;
    top: 8px;
    right: 16px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 3px 4px 3px 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .find input {
    width: 180px;
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    padding: 4px 2px;
  }
  .find.miss input {
    color: var(--st-failed);
  }
  .pane {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    background: var(--term-bg);
  }
  .term {
    flex: 1;
    min-width: 0;
    padding: 10px 0 0 14px;
    overflow: hidden;
    caret-color: transparent;
  }
  .pane.phone {
    flex-direction: column;
  }
  .phone .term {
    min-height: 0;
    padding: 6px 0 0 6px;
    touch-action: pan-y;
  }
  .fixed .term {
    overflow: auto;
    touch-action: pan-x pan-y;
  }
  .term :global(textarea) {
    caret-color: transparent;
    background: transparent;
    color: transparent;
    border: 0;
    border-radius: 0;
    padding: 0;
    resize: none;
    overflow: hidden;
    box-shadow: none;
  }
  .term :global(*:focus-visible) {
    outline: none;
  }
  .note {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .error {
    color: var(--st-failed);
  }
  .drop {
    position: absolute;
    inset: 4px;
    display: grid;
    place-items: center;
    border: 1px dashed var(--accent);
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--term-bg) 60%, transparent);
    pointer-events: none;
  }
  .drop span {
    padding: 4px 10px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 12px;
    color: var(--text-2);
  }
  .sized {
    position: absolute;
    top: 8px;
    right: 12px;
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-2);
    font-size: 11px;
    cursor: pointer;
  }
  .phone .sized {
    min-height: 32px;
    font-size: 12px;
  }
  .fit {
    color: var(--accent);
    font-weight: 500;
  }
  .copy {
    position: absolute;
    transform: translate(-50%, -130%);
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 44px;
    padding: 0 14px;
    border: none;
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
    font-size: 14px;
  }
  .picker {
    display: none;
  }
  .latest {
    position: absolute;
    left: 50%;
    bottom: 12px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    box-shadow: var(--shadow);
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
  }
  .phone .latest {
    bottom: 56px;
    min-height: 32px;
  }
  .exited {
    position: absolute;
    left: 50%;
    bottom: 12px;
    transform: translateX(-50%);
    padding: 4px 10px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-size: 12px;
    color: var(--text-2);
  }
</style>
