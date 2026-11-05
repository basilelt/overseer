const HEX3 = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i;
const HEX6 = /^#[0-9a-f]{6}$/i;
const RGB = /^rgba?\(\s*(\d{1,3})[\s,]+(\d{1,3})[\s,]+(\d{1,3})/i;

const hex2 = (n: number) => Math.min(255, n).toString(16).padStart(2, '0');

export function toHex6(value: string): string {
  const v = value.trim();
  if (HEX6.test(v)) return v.toLowerCase();
  const short = HEX3.exec(v);
  if (short) return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toLowerCase();
  const rgb = RGB.exec(v);
  if (rgb) return `#${hex2(+rgb[1])}${hex2(+rgb[2])}${hex2(+rgb[3])}`;
  if (/^[a-z]+$/i.test(v) && typeof document !== 'undefined') {
    const probe = document.createElement('span');
    probe.style.color = v;
    document.body.append(probe);
    const resolved = getComputedStyle(probe).color;
    probe.remove();
    if (resolved !== v) return toHex6(resolved);
  }
  return '#000000';
}
