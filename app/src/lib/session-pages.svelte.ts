import { isTauri } from './client';
import { nativePage, type Bounds, type NavEvent } from './browser';
import { attach, closePage, loadPages, MAX_LIVE, pageIds, patchPage, prune, savePages, select, setOpen, setRatio, touch, type Page, type PagesByKey } from './session-pages';

let seq = 0;
const newId = () => `p${Date.now().toString(36)}${(seq++).toString(36)}`;

export class SessionPages {
  host = '';
  byKey = $state.raw<PagesByKey>({});
  loading = $state.raw<ReadonlySet<string>>(new Set());
  live: string[] = [];
  private shown = new Set<string>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private ready: Promise<unknown> | null = null;

  constructor(
    private native = isTauri(),
    private max = MAX_LIVE,
  ) {}

  useHost(host: string) {
    if (host === this.host) return;
    this.flush();
    for (const id of [...this.live]) this.drop(id);
    this.host = host;
    this.byKey = loadPages(host);
  }

  page(id: string): Page | undefined {
    for (const w of Object.values(this.byKey)) {
      const p = w.pages.find((x) => x.id === id);
      if (p) return p;
    }
  }

  attach(key: string, url: string): Page {
    const r = attach(this.byKey, key, url, newId);
    this.set(r.state);
    return r.page;
  }

  close(key: string, id: string) {
    this.set(closePage(this.byKey, key, id));
  }

  select(key: string, id: string) {
    this.set(select(this.byKey, key, id));
  }

  setOpen(key: string, open: boolean) {
    this.set(setOpen(this.byKey, key, open));
  }

  setRatio(key: string, ratio: number) {
    this.set(setRatio(this.byKey, key, ratio));
  }

  prune(alive: ReadonlySet<string>) {
    this.set(prune(this.byKey, alive));
  }

  flush() {
    clearTimeout(this.timer);
    this.timer = undefined;
    if (this.host) savePages(this.host, this.byKey);
  }

  show(id: string, b: Bounds): Promise<unknown> {
    const page = this.page(id);
    if (!this.native || !page) return Promise.resolve();
    this.shown.add(id);
    const wasLive = this.live.includes(id);
    const t = touch(this.live, id, this.max);
    const keep = t.evicted.filter((x) => this.shown.has(x));
    this.live = [...keep, ...t.live];
    for (const x of t.evicted) if (!keep.includes(x)) this.drop(x);
    if (wasLive) return nativePage.show(id, b);
    this.setLoading(id, true);
    this.ready ??= nativePage.reset().catch(() => {});
    return this.ready
      .then(() => nativePage.open(id, page.url, b, (e) => this.onNav(id, e)))
      .then(() => {
        if (!this.shown.has(id)) return nativePage.hide(id);
      })
      .catch((e) => {
        this.live = this.live.filter((x) => x !== id);
        this.setLoading(id, false);
        throw e;
      });
  }

  bounds(id: string, b: Bounds) {
    if (this.native && this.shown.has(id) && this.live.includes(id)) nativePage.bounds(id, b).catch(() => {});
  }

  hide(id: string) {
    this.shown.delete(id);
    if (this.native && this.live.includes(id)) nativePage.hide(id).catch(() => {});
  }

  navigate(id: string, url: string): Promise<unknown> {
    this.set(patchPage(this.byKey, id, { url }));
    if (!this.native || !this.live.includes(id)) return Promise.resolve();
    this.setLoading(id, true);
    return nativePage.navigate(id, url);
  }

  step(id: string, delta: number): Promise<unknown> {
    if (!this.native || !this.live.includes(id)) return Promise.resolve();
    return delta < 0 ? nativePage.back(id) : nativePage.forward(id);
  }

  reload(id: string): Promise<unknown> {
    if (!this.native || !this.live.includes(id)) return Promise.resolve();
    return nativePage.reload(id);
  }

  onNav(id: string, e: NavEvent) {
    if (e.url) this.set(patchPage(this.byKey, id, e.title ? { url: e.url, title: e.title } : { url: e.url }));
    if (e.loading !== undefined) this.setLoading(id, e.loading);
  }

  private setLoading(id: string, on: boolean) {
    if (this.loading.has(id) === on) return;
    const next = new Set(this.loading);
    if (on) next.add(id);
    else next.delete(id);
    this.loading = next;
  }

  private drop(id: string) {
    this.live = this.live.filter((x) => x !== id);
    this.shown.delete(id);
    this.setLoading(id, false);
    if (this.native) nativePage.close(id).catch(() => {});
  }

  private set(next: PagesByKey) {
    if (next === this.byKey) return;
    const ids = pageIds(next);
    for (const id of [...this.live]) if (!ids.has(id)) this.drop(id);
    this.byKey = next;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), 500);
  }
}

export const pages = new SessionPages();
