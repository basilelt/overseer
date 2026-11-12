import { KEY_DEMO, KEY_DEMO_PREFIX, localStore } from '../storage';

export const DEMO_KEY = KEY_DEMO;
export const DEMO_PREFIX = KEY_DEMO_PREFIX;

let real: Storage | undefined;

function storage(): Storage | undefined {
  try {
    return real ?? localStore();
  } catch {}
}

export function isDemo(): boolean {
  try {
    return storage()?.getItem(DEMO_KEY) === '1';
  } catch {
    return false;
  }
}

function scopedKeys(base: Storage, prefix: string): string[] {
  const out: string[] = [];
  for (let i = 0; i < base.length; i++) {
    const k = base.key(i);
    if (k?.startsWith(prefix)) out.push(k.slice(prefix.length));
  }
  return out;
}

export function scopedStorage(base: Storage, prefix = DEMO_PREFIX): Storage {
  return {
    get length() {
      return scopedKeys(base, prefix).length;
    },
    key: (i: number) => scopedKeys(base, prefix)[i] ?? null,
    getItem: (k: string) => base.getItem(prefix + k),
    setItem: (k: string, v: string) => base.setItem(prefix + k, String(v)),
    removeItem: (k: string) => base.removeItem(prefix + k),
    clear: () => {
      for (const k of scopedKeys(base, prefix)) base.removeItem(prefix + k);
    },
  };
}

export function installDemoStorage(target: { localStorage?: Storage } = globalThis): boolean {
  try {
    const base = target.localStorage;
    if (base && isDemo()) {
      Object.defineProperty(target, 'localStorage', { value: scopedStorage(base), configurable: true, writable: true });
      real = base;
      return true;
    }
    if (base) scopedStorage(base).clear();
  } catch {}
  return false;
}

export function uninstallDemoStorage(target: { localStorage?: Storage } = globalThis) {
  if (!real) return;
  Object.defineProperty(target, 'localStorage', { value: real, configurable: true, writable: true });
  real = undefined;
}

const reloadPage = () => globalThis.location?.reload();

export function enterDemo(reload: () => void = reloadPage) {
  try {
    storage()?.setItem(DEMO_KEY, '1');
  } catch {
    return;
  }
  reload();
}

export async function startDemo(client: { disconnect(): Promise<void> } | null | undefined, reload: () => void = reloadPage) {
  try {
    await client?.disconnect();
  } catch {}
  enterDemo(reload);
}

export function exitDemo(reload: () => void = reloadPage, target: { localStorage?: Storage } = globalThis) {
  const base = storage();
  try {
    if (base) {
      scopedStorage(base).clear();
      base.removeItem(DEMO_KEY);
    }
    if (real) {
      const none = () => null;
      Object.defineProperty(target, 'localStorage', { value: { length: 0, key: none, getItem: none, setItem: none, removeItem: none, clear: none }, configurable: true, writable: true });
    }
  } catch {}
  reload();
}
