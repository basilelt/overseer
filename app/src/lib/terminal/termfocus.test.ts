import { afterEach, describe, expect, it } from 'vitest';
import { keepInputFocus } from './termfocus';

class Doc {
  activeElement: Node | null = null;
}

class Node extends EventTarget {
  constructor(
    readonly name: string,
    private doc: Doc,
  ) {
    super();
  }
  focus(_opts?: FocusOptions) {
    if (this.doc.activeElement === this) return;
    this.doc.activeElement = this;
    this.dispatchEvent(new Event('focus'));
  }
}

let stop: (() => void) | null = null;

afterEach(() => {
  stop?.();
  stop = null;
});

function setup() {
  const doc = new Doc();
  const container = new Node('container', doc);
  const input = new Node('textarea', doc);
  stop = keepInputFocus(container, input, doc);
  return { doc, container, input };
}

describe('keepInputFocus', () => {
  it('moves focus from the contenteditable container to the input textarea, where a tap puts it', () => {
    const { doc, container, input } = setup();
    container.focus();
    expect(doc.activeElement).toBe(input);
  });

  it('takes focus from the container when it already has it, as ghostty-web open() leaves it', () => {
    const doc = new Doc();
    const container = new Node('container', doc);
    const input = new Node('textarea', doc);
    container.focus();
    stop = keepInputFocus(container, input, doc);
    expect(doc.activeElement).toBe(input);
  });

  it('leaves focus on the input when it is focused directly', () => {
    const { doc, input } = setup();
    input.focus();
    expect(doc.activeElement).toBe(input);
  });

  it('stops redirecting once disposed', () => {
    const { doc, container } = setup();
    stop?.();
    stop = null;
    container.focus();
    expect(doc.activeElement).toBe(container);
  });

  it('does nothing without an input', () => {
    const doc = new Doc();
    const container = new Node('container', doc);
    stop = keepInputFocus(container, null, doc);
    container.focus();
    expect(doc.activeElement).toBe(container);
  });
});
