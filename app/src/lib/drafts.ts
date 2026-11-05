export interface DraftStorage {
  getItem(k: string): string | null;
  setItem(k: string, v: string): void;
  removeItem(k: string): void;
}

export interface DraftOptions {
  max?: number;
  maxLength?: number;
  delay?: number;
  storage?: DraftStorage | null;
  timers?: { set: (fn: () => void, ms: number) => unknown; clear: (id: unknown) => void };
}

const all = new Set<Drafts<unknown>>();
let hooked = false;

function hookPageHide() {
  if (hooked || typeof window === 'undefined') return;
  hooked = true;
  const flush = () => all.forEach((d) => d.flush());
  window.addEventListener('pagehide', flush);
  document.addEventListener('visibilitychange', () => document.visibilityState === 'hidden' && flush());
}

const storageOf = (): DraftStorage | null => {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
};

export class Drafts<T = string> {
  private map: Map<string, T> | null = null;
  private timer: unknown = null;
  private readonly max: number;
  private readonly maxLength: number;
  private readonly delay: number;
  private readonly storage: DraftStorage | null;
  private readonly timers: NonNullable<DraftOptions['timers']>;

  constructor(
    private readonly key: string,
    private readonly empty: (v: T) => boolean = (v) => !v,
    opts: DraftOptions = {},
  ) {
    this.max = opts.max ?? 50;
    this.maxLength = opts.maxLength ?? 20_000;
    this.delay = opts.delay ?? 400;
    this.storage = opts.storage === undefined ? storageOf() : opts.storage;
    this.timers = opts.timers ?? { set: (fn, ms) => setTimeout(fn, ms), clear: (id) => clearTimeout(id as ReturnType<typeof setTimeout>) };
    all.add(this as Drafts<unknown>);
    hookPageHide();
  }

  private load(): Map<string, T> {
    if (this.map) return this.map;
    this.map = new Map();
    try {
      const raw = JSON.parse(this.storage?.getItem(this.key) ?? '[]');
      if (Array.isArray(raw)) for (const e of raw) if (Array.isArray(e) && typeof e[0] === 'string') this.map.set(e[0], e[1] as T);
    } catch {}
    return this.map;
  }

  get(id: string): T | undefined {
    return this.load().get(id);
  }

  set(id: string, value: T) {
    const m = this.load();
    const prev = m.get(id);
    if (this.empty(value)) {
      if (!m.delete(id)) return;
    } else {
      if (prev === value) return;
      m.delete(id);
      m.set(id, value);
      while (m.size > this.max) m.delete(m.keys().next().value as string);
    }
    this.schedule();
  }

  delete(id: string) {
    if (this.load().delete(id)) this.schedule();
  }

  private schedule() {
    if (this.timer !== null) return;
    this.timer = this.timers.set(() => this.flush(), this.delay);
  }

  flush() {
    if (this.timer !== null) this.timers.clear(this.timer);
    this.timer = null;
    if (!this.map || !this.storage) return;
    const entries = [...this.map].filter(([, v]) => JSON.stringify(v).length <= this.maxLength);
    try {
      if (entries.length) this.storage.setItem(this.key, JSON.stringify(entries));
      else this.storage.removeItem(this.key);
    } catch {}
  }
}
