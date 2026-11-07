import { Ghostty, InputHandler } from 'ghostty-web';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';
import { attachIme, imeBytes } from './ime';

class Field extends EventTarget {
  value = '';
}

let ghostty: Ghostty;
let root: Field;
let sent: string[];
let handler: InputHandler;

beforeAll(async () => {
  const fsModule: string = 'node:fs';
  const fs: { readFileSync(path: URL): Uint8Array<ArrayBuffer> } = await import(/* @vite-ignore */ fsModule);
  const bytes = fs.readFileSync(new URL('../../node_modules/ghostty-web/dist/ghostty-vt.wasm', import.meta.url));
  let memory: WebAssembly.Memory | undefined;
  const { instance } = await WebAssembly.instantiate(bytes, {
    env: { log: (p: number, n: number) => console.log(new TextDecoder().decode(new Uint8Array(memory!.buffer, p, n))) },
  });
  memory = instance.exports.memory as WebAssembly.Memory;
  ghostty = new Ghostty(instance);
});

beforeEach(() => {
  root = new Field();
  sent = [];
  const out = (d: string) => sent.push(d);
  handler = new InputHandler(ghostty, root as unknown as HTMLElement, out, () => {}, undefined, undefined, () => false);
  attachIme(root, out);
});

function fire(type: string, props: Record<string, unknown> = {}, cancelable = true): Event {
  const e = Object.assign(new Event(type, { cancelable }), props);
  root.dispatchEvent(e);
  return e;
}

const imeKey = () => ({ key: 'Unidentified', code: '', keyCode: 229 });

function softType(text: string) {
  fire('keydown', imeKey());
  fire('beforeinput', { inputType: 'insertText', data: text });
  fire('keyup', imeKey());
}

function compose(steps: string[], commit: string) {
  fire('compositionstart', { data: '' });
  for (const s of steps) {
    fire('keydown', { ...imeKey(), isComposing: true });
    fire('beforeinput', { inputType: 'insertCompositionText', data: s, isComposing: true }, false);
    fire('compositionupdate', { data: s });
    root.value = s;
    fire('input', { inputType: 'insertCompositionText', data: s, isComposing: true }, false);
    fire('keyup', { ...imeKey(), isComposing: true });
  }
  fire('compositionend', { data: commit });
}

describe('imeBytes', () => {
  it('maps input types to terminal bytes', () => {
    expect(imeBytes({ inputType: 'insertText', data: 'a' })).toBe('a');
    expect(imeBytes({ inputType: 'insertLineBreak', data: null })).toBe('\r');
    expect(imeBytes({ inputType: 'insertParagraph', data: null })).toBe('\r');
    expect(imeBytes({ inputType: 'deleteContentBackward', data: null })).toBe('\x7f');
    expect(imeBytes({ inputType: 'deleteContentForward', data: null })).toBe('\x1b[3~');
    expect(imeBytes({ inputType: 'deleteWordBackward', data: null })).toBe('\x17');
    expect(imeBytes({ inputType: 'insertReplacementText', data: 'the', replaced: 3 })).toBe('\x7f\x7f\x7fthe');
  });

  it('leaves composition, paste and drops to their own paths', () => {
    expect(imeBytes({ inputType: 'insertCompositionText', data: 'he', isComposing: true })).toBeNull();
    expect(imeBytes({ inputType: 'deleteContentBackward', data: null, isComposing: true })).toBeNull();
    expect(imeBytes({ inputType: 'insertText', data: 'x', isComposing: true })).toBeNull();
    expect(imeBytes({ inputType: 'insertFromPaste', data: 'x' })).toBeNull();
    expect(imeBytes({ inputType: 'insertFromDrop', data: 'x' })).toBeNull();
    expect(imeBytes({ inputType: 'insertText', data: '' })).toBeNull();
  });
});

describe('soft keyboard (Android / iOS)', () => {
  it('sends letters typed as keydown 229 + beforeinput insertText', () => {
    for (const ch of 'hi there') softType(ch);
    expect(sent.join('')).toBe('hi there');
  });

  it('cancels the insertion so the hidden field stays empty', () => {
    fire('keydown', imeKey());
    const e = fire('beforeinput', { inputType: 'insertText', data: 'a' });
    expect(e.defaultPrevented).toBe(true);
  });

  it('sends a Gboard composition once, on commit, then the space', () => {
    compose(['h', 'he', 'hel', 'he', 'hey'], 'hey');
    fire('keydown', { ...imeKey(), isComposing: true });
    fire('beforeinput', { inputType: 'deleteContentBackward', data: null, isComposing: true }, false);
    softType(' ');
    expect(sent).toEqual(['hey', ' ']);
    expect(root.value).toBe('');
  });

  it('sends Backspace from deleteContentBackward with no keydown', () => {
    fire('beforeinput', { inputType: 'deleteContentBackward', data: null });
    expect(sent).toEqual(['\x7f']);
  });

  it('sends Backspace once when the keyboard also sends a real keydown', () => {
    fire('keydown', { key: 'Backspace', code: 'Backspace', keyCode: 8 });
    fire('beforeinput', { inputType: 'deleteContentBackward', data: null });
    fire('keyup', { key: 'Backspace', code: 'Backspace', keyCode: 8 });
    expect(sent).toEqual(['\x7f']);
  });

  it('keeps sending Backspace when a composition never ended and the field is empty', () => {
    fire('compositionstart', { data: '' });
    const bs = { key: 'Backspace', code: 'Backspace', keyCode: 8 };
    for (let i = 0; i < 3; i++) {
      const e = fire('keydown', bs);
      if (!e.defaultPrevented && i === 0) fire('beforeinput', { inputType: 'deleteContentBackward', data: null });
      fire('keyup', bs);
    }
    expect(sent).toEqual(['\x7f', '\x7f', '\x7f']);
  });

  it('leaves Backspace to the IME while a composition is really open', () => {
    fire('compositionstart', { data: '' });
    fire('keydown', { key: 'Backspace', code: 'Backspace', keyCode: 229, isComposing: true });
    fire('keydown', { key: 'Backspace', code: 'Backspace', keyCode: 8, isComposing: true });
    expect(sent).toEqual([]);
  });

  it('sends Enter from insertLineBreak and from a real Enter keydown', () => {
    fire('keydown', imeKey());
    fire('beforeinput', { inputType: 'insertLineBreak', data: null });
    fire('keyup', imeKey());
    fire('keydown', { key: 'Enter', code: 'Enter', keyCode: 13 });
    fire('beforeinput', { inputType: 'insertLineBreak', data: null });
    fire('keyup', { key: 'Enter', code: 'Enter', keyCode: 13 });
    expect(sent).toEqual(['\r', '\r']);
  });

  it('replaces an autocorrected word by erasing the target range', () => {
    const node = {};
    fire('beforeinput', {
      inputType: 'insertReplacementText',
      data: null,
      dataTransfer: { getData: () => 'the' },
      getTargetRanges: () => [{ startContainer: node, endContainer: node, startOffset: 0, endOffset: 3 }],
    });
    expect(sent).toEqual(['\x7f\x7f\x7fthe']);
  });

  it('sends suggestion-bar insertions that come without any keydown', () => {
    fire('beforeinput', { inputType: 'insertText', data: 'hello ' });
    expect(sent).toEqual(['hello ']);
  });

  it('empties the hidden field after a commit', () => {
    root.value = 'stale';
    fire('input', { inputType: 'insertText', data: 'x', isComposing: false }, false);
    expect(root.value).toBe('');
    root.value = 'word';
    fire('compositionend', { data: 'word' });
    expect(root.value).toBe('');
  });
});

describe('physical keyboard (unchanged)', () => {
  it('sends printable keys once through ghostty-web', () => {
    for (const [key, code] of [
      ['a', 'KeyA'],
      ['B', 'KeyB'],
      ['1', 'Digit1'],
    ]) {
      fire('keydown', { key, code, keyCode: key.toUpperCase().charCodeAt(0) });
      fire('beforeinput', { inputType: 'insertText', data: key });
      fire('keyup', { key, code });
    }
    expect(sent).toEqual(['a', 'B', '1']);
  });

  it('encodes special keys as before', () => {
    fire('keydown', { key: 'Enter', code: 'Enter', keyCode: 13 });
    fire('keydown', { key: 'Backspace', code: 'Backspace', keyCode: 8 });
    fire('keydown', { key: 'Tab', code: 'Tab', keyCode: 9 });
    fire('keydown', { key: 'Escape', code: 'Escape', keyCode: 27 });
    fire('keydown', { key: 'ArrowUp', code: 'ArrowUp', keyCode: 38 });
    fire('keydown', { key: 'c', code: 'KeyC', keyCode: 67, ctrlKey: true });
    expect(sent).toEqual(['\r', '\x7f', '\t', '\x1b', '\x1b[A', '\x03']);
  });

  it('keeps desktop IME commits (CJK, dead keys) on compositionend only', () => {
    compose(['n', 'ni', 'nih', '你'], '你');
    expect(sent).toEqual(['你']);
  });

  it('leaves paste to the paste handlers', () => {
    fire('keydown', { key: 'v', code: 'KeyV', keyCode: 86, ctrlKey: true });
    fire('beforeinput', { inputType: 'insertFromPaste', data: null });
    expect(sent).toEqual([]);
  });

  it('stops listening when detached', () => {
    handler.dispose();
    const r = new Field();
    const out: string[] = [];
    const stop = attachIme(r, (d) => out.push(d));
    stop();
    r.dispatchEvent(Object.assign(new Event('beforeinput', { cancelable: true }), { inputType: 'insertText', data: 'a' }));
    expect(out).toEqual([]);
  });
});
