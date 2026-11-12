export class FakeEl {
  parent: FakeEl | null = null;
  isContentEditable = false;
  type = '';
  constructor(
    public tagName: string,
    public attrs: Record<string, string> = {},
    public textContent = '',
  ) {
    this.type = attrs.type ?? '';
  }
  getAttribute(k: string): string | null {
    return this.attrs[k] ?? null;
  }
  in(parent: FakeEl): this {
    this.parent = parent;
    return this;
  }
  matches(sel: string): boolean {
    return sel.split(',').some((raw) => {
      const s = raw.trim();
      const attr = /^\[([\w-]+)(?:="([^"]*)")?\]$/.exec(s);
      if (attr) return attr[2] === undefined ? attr[1] in this.attrs : this.attrs[attr[1]] === attr[2];
      if (s.startsWith('.')) return (this.attrs.class ?? '').split(' ').includes(s.slice(1));
      return this.tagName.toLowerCase() === s;
    });
  }
  closest(sel: string): FakeEl | null {
    for (let el: FakeEl | null = this; el; el = el.parent) if (el.matches(sel)) return el;
    return null;
  }
}

export const el = (tag: string, attrs: Record<string, string> = {}, text = '') => new FakeEl(tag.toUpperCase(), attrs, text);
