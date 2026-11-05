import { describe, expect, it } from 'vitest';
import { build, type Rolldown } from 'vite';

const ENTRY_BUDGET_KB = 340;
const STARTUP_BUDGET_KB = 530;
const LAZY_ONLY = /^(hljs|hljs-core|mock|ReviewView|ReviewTab|KanbanView|ChatView|MobileShell|MobileConnect|SettingsDialog|GraphTabKind|ProfilesDialog|CommandPalette|UpdateDialog|ApkUpdateSheet|ApkUpdateHost|apk-update\.svelte)$/;

async function chunks(): Promise<Rolldown.OutputChunk[]> {
  const out = await build({ configFile: 'vite.config.ts', mode: 'production', logLevel: 'silent', build: { write: false, reportCompressedSize: false } });
  const result = (Array.isArray(out) ? out[0] : out) as Rolldown.RolldownOutput;
  return result.output.filter((o): o is Rolldown.OutputChunk => o.type === 'chunk');
}

describe('production bundle budget', () => {
  it('keeps the startup JS small and heavy views lazy', { timeout: 120_000 }, async () => {
    const all = await chunks();
    const byFile = new Map(all.map((c) => [c.fileName, c]));
    const entry = all.find((c) => c.isEntry)!;
    const startup = new Set<string>();
    const visit = (c: Rolldown.OutputChunk) => {
      if (startup.has(c.fileName)) return;
      startup.add(c.fileName);
      for (const f of c.imports) visit(byFile.get(f)!);
    };
    visit(entry);
    const kb = (c: Rolldown.OutputChunk) => c.code.length / 1024;
    const total = [...startup].reduce((s, f) => s + kb(byFile.get(f)!), 0);
    expect(kb(entry)).toBeLessThan(ENTRY_BUDGET_KB);
    expect(total).toBeLessThan(STARTUP_BUDGET_KB);
    expect([...startup].map((f) => byFile.get(f)!.name).filter((n) => LAZY_ONLY.test(n))).toEqual([]);
    expect([...startup].flatMap((f) => byFile.get(f)!.moduleIds).filter((m) => /\/i18n\/(?!en\.ts)[^/]+\.ts$/.test(m))).toEqual([]);
    expect(all.filter((c) => !startup.has(c.fileName)).some((c) => c.moduleIds.some((m) => m.endsWith('/i18n/fr.ts')))).toBe(true);
  });
});
