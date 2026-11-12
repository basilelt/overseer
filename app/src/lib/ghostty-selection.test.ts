import { beforeAll, describe, expect, it } from 'vitest';
import source from 'ghostty-web?raw';
import { patchGhostty } from '../../vite.config';

type Mod = typeof import('ghostty-web');
type Wasm = ReturnType<InstanceType<Mod['Ghostty']>['createTerminal']>;

let mod: Mod;
let wasm: WebAssembly.Module;

beforeAll(async () => {
  const fsModule = 'node:fs/promises';
  const { readFile } = (await import(/* @vite-ignore */ fsModule)) as { readFile(path: URL): Promise<Uint8Array<ArrayBuffer>> };
  wasm = await WebAssembly.compile(await readFile(new URL('../../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url)));
  const url = `data:text/javascript;base64,${Buffer.from(patchGhostty(source)).toString('base64')}`;
  mod = (await import(/* @vite-ignore */ url)) as Mod;
});

async function term(cols: number, rows: number, text: string): Promise<Wasm> {
  const g = new mod.Ghostty(await WebAssembly.instantiate(wasm, { env: { log: () => {} } }));
  const t = g.createTerminal(cols, rows, { scrollbackLimit: 1000 });
  t.write(text);
  t.update();
  return t;
}

function select(t: Wasm, from: [number, number], to: [number, number]): string {
  const sb = t.getScrollbackLength();
  const self = { wasmTerm: t, selectionStart: { col: from[0], absoluteRow: sb + from[1] }, selectionEnd: { col: to[0], absoluteRow: sb + to[1] } };
  return mod.SelectionManager.prototype.getSelection.call(self as unknown as InstanceType<Mod['SelectionManager']>);
}

describe('terminal selection text', () => {
  it('joins a soft-wrapped line instead of breaking it at the right margin', async () => {
    const t = await term(10, 5, '0123456789abcdefghijKLM\r\nshort\r\n');
    expect(select(t, [0, 0], [9, 3])).toBe('0123456789abcdefghijKLM\nshort');
  });

  it('keeps the space where a wrapped line broke after it', async () => {
    const t = await term(5, 5, 'abcd efgh ijkl\r\n');
    expect(select(t, [0, 0], [4, 2])).toBe('abcd efgh ijkl');
  });

  it('still breaks lines the program ended itself and trims their trailing blanks', async () => {
    const t = await term(10, 5, 'one  \r\ntwo\r\n');
    expect(select(t, [0, 0], [9, 1])).toBe('one\ntwo');
  });
});
