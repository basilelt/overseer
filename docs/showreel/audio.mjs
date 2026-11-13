import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const MODE = process.argv[3] ?? '15';
if (MODE !== '15' && MODE !== '30') {
  console.error('usage: node audio.mjs [out.wav] [15|30]');
  process.exit(1);
}
const S = MODE === '30' ? 2 : 1;
const SR = 48000;
const N = 720000 * S;
const B = 0.46875;
const OUT = resolve(process.argv[2] ?? resolve(dirname(fileURLToPath(import.meta.url)), '../build/reel.wav'));
const TARGET_LUFS = -16;
const CEILING = Math.pow(10, -1.3 / 20);

const TAU = Math.PI * 2;
const clamp = (x, a, b) => Math.min(b, Math.max(a, x));
const smooth = (x) => {
  x = clamp(x, 0, 1);
  return x * x * (3 - 2 * x);
};
const ns = (t) => Math.round(t * SR);
const hz = (midi) => 440 * Math.pow(2, (midi - 69) / 12);

function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const bus = () => ({ L: new Float64Array(N), R: new Float64Array(N) });
const drums = bus();
const music = bus();
const fx = bus();
const room = bus();
const hall = bus();
const echo = bus();

const panL = (p) => Math.cos(((p + 1) * Math.PI) / 4) * Math.SQRT2;
const panR = (p) => Math.sin(((p + 1) * Math.PI) / 4) * Math.SQRT2;

function svf() {
  let a = 0;
  let b = 0;
  const f = (x, fc, q = 0.707) => {
    const g = Math.tan((Math.PI * clamp(fc, 12, SR * 0.45)) / SR);
    const k = 1 / q;
    const a1 = 1 / (1 + g * (g + k));
    const a2 = g * a1;
    const a3 = g * a2;
    const v3 = x - b;
    const v1 = a1 * a + a2 * v3;
    const v2 = b + a2 * a + a3 * v3;
    a = 2 * v1 - a;
    b = 2 * v2 - b;
    f.lp = v2;
    f.bp = v1;
    f.hp = x - k * v1 - v2;
    return v2;
  };
  return f;
}

function blep(p, dt) {
  if (p < dt) {
    p /= dt;
    return p + p - p * p - 1;
  }
  if (p > 1 - dt) {
    p = (p - 1) / dt;
    return p * p + p + p + 1;
  }
  return 0;
}

function voice(t0, dur, gen, o = {}) {
  const { dest = fx, pan = 0, gain = 1, hall: h = 0, room: rm = 0, echo: e = 0, fadeIn = 0.001, fadeOut = 0.01 } = o;
  const n0 = ns(t0);
  const len = ns(dur);
  const dyn = typeof pan === 'function';
  let gl = panL(dyn ? 0 : pan) * gain;
  let gr = panR(dyn ? 0 : pan) * gain;
  for (let i = 0; i < len; i++) {
    const t = i / SR;
    let y = gen(t, i);
    if (t < fadeIn) y *= t / fadeIn;
    const rem = dur - t;
    if (rem < fadeOut) y *= Math.max(0, rem / fadeOut);
    const n = n0 + i;
    if (n < 0 || n >= N) continue;
    if (dyn) {
      const p = clamp(pan(t), -1, 1);
      gl = panL(p) * gain;
      gr = panR(p) * gain;
    }
    const l = y * gl;
    const r = y * gr;
    dest.L[n] += l;
    dest.R[n] += r;
    if (h) {
      hall.L[n] += l * h;
      hall.R[n] += r * h;
    }
    if (rm) {
      room.L[n] += l * rm;
      room.R[n] += r * rm;
    }
    if (e) {
      echo.L[n] += l * e;
      echo.R[n] += r * e;
    }
  }
}

function kick(t0, g) {
  let ph = 0;
  const r = rng(ns(t0) ^ 0x51ed);
  const f = svf();
  voice(
    t0,
    0.44,
    (t) => {
      const fr = 49 + 128 * Math.exp(-t / 0.03) + 420 * Math.exp(-t / 0.003);
      ph += (TAU * fr) / SR;
      const env = Math.exp(-Math.max(0, t - 0.03) / 0.12);
      const y = Math.tanh(1.9 * Math.sin(ph) * env) / Math.tanh(1.9);
      f(r() * 2 - 1, 4000);
      return y * 0.92 + f.hp * Math.exp(-t / 0.0016) * 0.3;
    },
    { dest: drums, gain: g, fadeIn: 0.0004, fadeOut: 0.04 },
  );
}

function clap(t0, g) {
  const env = (t) => {
    let e = 0;
    for (let j = 0; j < 3; j++) {
      const d = t - j * 0.0095;
      if (d >= 0) e = Math.max(e, Math.exp(-d / 0.0032));
    }
    const d = t - 0.026;
    if (d >= 0) e = Math.max(e, 0.78 * Math.exp(-d / 0.085));
    return e;
  };
  for (const side of [-1, 1]) {
    const r = rng(ns(t0) * 7 + side * 131);
    const f = svf();
    const f2 = svf();
    voice(
      t0,
      0.4,
      (t) => {
        f(r() * 2 - 1, 1350, 0.9);
        f2(f.bp, 700);
        return f2.hp * env(t) * 1.6;
      },
      { dest: drums, gain: g, pan: side * 0.4, room: 0.32, hall: 0.05 },
    );
  }
  let ph = 0;
  voice(
    t0,
    0.12,
    (t) => {
      ph += (TAU * (175 + 60 * Math.exp(-t / 0.01))) / SR;
      return Math.sin(ph) * Math.exp(-t / 0.035) * 0.55;
    },
    { dest: drums, gain: g },
  );
}

function hat(t0, g, decay, pan, seed) {
  const r = rng(seed);
  const f = svf();
  const f2 = svf();
  const metal = [317, 487, 563, 769, 913, 1187].map((x) => x * 6.3);
  voice(
    t0,
    Math.min(0.9, decay * 7),
    (t) => {
      f(r() * 2 - 1, 7200, 0.6);
      let m = 0;
      for (const fr of metal) m += Math.sign(Math.sin(TAU * fr * t));
      f2(m / 6, 9000, 0.8);
      return (f.hp * 0.75 + f2.hp * 0.3) * Math.exp(-t / decay);
    },
    { dest: drums, gain: g, pan, room: 0.12, fadeIn: 0.0005, fadeOut: 0.01 },
  );
}

function crash(t0, g) {
  for (const side of [-1, 1]) {
    const r = rng(ns(t0) * 3 + side * 977);
    const f = svf();
    const f2 = svf();
    voice(
      t0,
      2.6,
      (t) => {
        f(r() * 2 - 1, 4200, 0.6);
        f2(f.hp, 8500, 0.7);
        return (f.hp * 0.6 + f2.bp * 0.5) * Math.exp(-t / 0.75) * (1 + 1.5 * Math.exp(-t / 0.02));
      },
      { dest: fx, gain: g, pan: side * 0.65, hall: 0.25, fadeIn: 0.0008, fadeOut: 0.3 },
    );
  }
}

function snare(t0, g, fc) {
  const r = rng(ns(t0) ^ 0x7777);
  const f = svf();
  let ph = 0;
  voice(
    t0,
    0.16,
    (t) => {
      f(r() * 2 - 1, fc, 0.8);
      ph += (TAU * (210 + 70 * Math.exp(-t / 0.008))) / SR;
      return (f.bp * 1.4 + Math.sin(ph) * 0.35) * Math.exp(-t / 0.045);
    },
    { dest: drums, gain: g, room: 0.25, fadeIn: 0.0005, fadeOut: 0.02 },
  );
}

function bassNote(t0, f, dur, g, bright = 1) {
  let p = 0;
  let ph = 0;
  const lp = svf();
  voice(
    t0,
    dur + 0.05,
    (t) => {
      const dt = f / SR;
      p += dt;
      if (p >= 1) p -= 1;
      ph += TAU * dt;
      const raw = (2 * p - 1 - blep(p, dt)) * 0.75 + Math.sin(ph) * 0.6;
      const y = lp(raw, 200 + bright * 1700 * Math.exp(-t / 0.06), 0.95);
      const env = t < dur ? 1 : Math.max(0, 1 - (t - dur) / 0.05);
      return Math.tanh(1.6 * y * env) / 1.2;
    },
    { dest: music, gain: g, fadeIn: 0.003, fadeOut: 0.02 },
  );
}

function chord(t0, t1, freqs, o = {}) {
  const { g = 1, att = 0.05, rel = 0.3, cut = () => 1800, res = 0.8, bend = () => 1, h = 0.22, voices = 6, cents = 13, seed = 7, dest = music } = o;
  const r = rng(seed);
  const osc = [];
  freqs.forEach((f, k) => {
    for (let v = 0; v < voices; v++) {
      const x = (v / (voices - 1)) * 2 - 1;
      osc.push({ f: f * Math.pow(2, (x * cents) / 1200), p: r(), side: (v + k) % 2 ? 0.82 : 0.18, w: 1 - Math.abs(x) * 0.35 });
    }
  });
  const fl = svf();
  const fr = svf();
  const n0 = Math.max(0, ns(t0));
  const n1 = Math.min(N, ns(t1 + rel));
  for (let n = n0; n < n1; n++) {
    const ta = n / SR;
    const t = ta - t0;
    const bf = bend(ta);
    let l = 0;
    let rr = 0;
    for (const v of osc) {
      const dt = (v.f * bf) / SR;
      v.p += dt;
      if (v.p >= 1) v.p -= 1;
      const s = (2 * v.p - 1 - blep(v.p, dt)) * v.w;
      l += s * v.side;
      rr += s * (1 - v.side);
    }
    const c = cut(ta);
    const yl = fl(l, c, res);
    const yr = fr(rr, c, res);
    const env = (t < att ? smooth(t / att) : 1) * (ta > t1 ? Math.pow(Math.max(0, 1 - (ta - t1) / rel), 2) : 1);
    dest.L[n] += yl * env * g;
    dest.R[n] += yr * env * g;
    hall.L[n] += yl * env * g * h;
    hall.R[n] += yr * env * g * h;
  }
}

function click(t0, g, fc = 5200, pan = 0) {
  const r = rng(ns(t0) ^ 0x3c3c);
  const f = svf();
  voice(
    t0,
    0.02,
    (t) => {
      f(r() * 2 - 1, fc, 1.2);
      return f.bp * Math.exp(-t / 0.0018) * 2;
    },
    { dest: fx, gain: g, pan, fadeIn: 0.0002, fadeOut: 0.004 },
  );
}

function keyClick(t0, i) {
  const r = rng(1000 + i);
  const fc = 3200 + r() * 2200;
  const body = 650 + r() * 450;
  const pan = (r() - 0.5) * 0.7;
  const g = 0.17 * (0.8 + r() * 0.4);
  click(t0, g, fc, pan);
  voice(t0, 0.03, (t) => Math.sin(TAU * body * t) * Math.exp(-t / 0.006), { dest: fx, gain: g * 0.45, pan, room: 0.2, fadeIn: 0.0003, fadeOut: 0.005 });
}

function stab(t0, freqs, g) {
  const r = rng(ns(t0));
  const osc = freqs.flatMap((f) => [-10, 0, 10].map((c) => ({ f: f * Math.pow(2, c / 1200), p: r() })));
  const fl = svf();
  voice(
    t0,
    0.55,
    (t) => {
      let s = 0;
      for (const o of osc) {
        const dt = o.f / SR;
        o.p += dt;
        if (o.p >= 1) o.p -= 1;
        s += 2 * o.p - 1 - blep(o.p, dt);
      }
      const y = fl(s / osc.length, 700 + 5600 * Math.exp(-t / 0.05), 1.1);
      return y * Math.exp(-t / 0.15) * 2.4;
    },
    { dest: fx, gain: g, hall: 0.3, echo: 0.35, fadeIn: 0.0008, fadeOut: 0.06 },
  );
  click(t0, g * 0.5, 6500);
}

function whoosh(t0, dur, o = {}) {
  const { f0 = 300, f1 = 3000, g = 0.2, p0 = -0.6, p1 = 0.6, q = 1.1, peak = 0.6, seed = 1, h = 0.25, air = 0.3 } = o;
  const r = rng(seed);
  const f = svf();
  const fa = svf();
  voice(
    t0,
    dur,
    (t) => {
      const u = t / dur;
      const fc = f0 * Math.pow(f1 / f0, smooth(u));
      const x = r() * 2 - 1;
      f(x, fc, q);
      fa(x, 6000);
      const env = u < peak ? Math.pow(u / peak, 2) : Math.pow(Math.max(0, 1 - (u - peak) / (1 - peak)), 1.7);
      return (f.bp + fa.hp * air * smooth(u * 1.4)) * env;
    },
    { dest: fx, gain: g, pan: (t) => p0 + (p1 - p0) * (t / dur), hall: h, fadeIn: 0.002, fadeOut: 0.01 },
  );
}

function riser(t0, t1, o = {}) {
  const { g = 0.15, f0 = 400, f1 = 7000, tone = 0.25, p0 = 220, p1 = 880, seed = 9, h = 0.2 } = o;
  const dur = t1 - t0;
  const r = rng(seed);
  const f = svf();
  let ph = 0;
  voice(
    t0,
    dur,
    (t) => {
      const u = t / dur;
      const fc = f0 * Math.pow(f1 / f0, u);
      f(r() * 2 - 1, fc, 1.4);
      ph += (TAU * p0 * Math.pow(p1 / p0, u * u)) / SR;
      const tri = (2 / Math.PI) * Math.asin(Math.sin(ph));
      return (f.bp + tri * tone * 0.5) * Math.pow(u, 2.2);
    },
    { dest: fx, gain: g, hall: h, fadeIn: 0.003, fadeOut: 0.004 },
  );
}

function boom(t0, g, o = {}) {
  const { f0 = 110, f1 = 36, tp = 0.09, ta = 0.8, noise = 0.35, nd = 0.25, nf = 2400, h = 0.35 } = o;
  let ph = 0;
  voice(
    t0,
    ta * 5,
    (t) => {
      ph += (TAU * (f1 + (f0 - f1) * Math.exp(-t / tp))) / SR;
      return Math.tanh(2 * Math.sin(ph) * Math.exp(-t / ta)) * 0.9;
    },
    { dest: fx, gain: g, hall: h * 0.4, fadeIn: 0.0008, fadeOut: 0.3 },
  );
  for (const side of [-1, 1]) {
    const r = rng(ns(t0) + side * 4421);
    const f = svf();
    voice(
      t0,
      nd * 6,
      (t) => {
        f(r() * 2 - 1, nf * (0.4 + 0.6 * Math.exp(-t / 0.15)), 0.7);
        return f.lp * Math.exp(-t / nd) * 1.8;
      },
      { dest: fx, gain: g * noise, pan: side * 0.5, hall: h, fadeIn: 0.0008, fadeOut: 0.2 },
    );
  }
}

function bell(t0, f, g, o = {}) {
  const { decay = 0.45, ratio = 3.5, index = 2.0, pan = 0, h = 0.3, e = 0.2 } = o;
  voice(
    t0,
    decay * 4.5,
    (t) => {
      const I = index * Math.exp(-t / (decay * 0.3));
      return Math.sin(TAU * f * t + I * Math.sin(TAU * f * ratio * t)) * Math.exp(-t / decay);
    },
    { dest: fx, gain: g, pan, hall: h, echo: e, fadeIn: 0.0015, fadeOut: 0.08 },
  );
}

function ping(t0, f, g, o = {}) {
  const { decay = 0.12, pan = 0, h = 0.35, e = 0.3 } = o;
  voice(
    t0,
    decay * 6,
    (t) => (Math.sin(TAU * f * t) + 0.3 * Math.sin(TAU * f * 2.01 * t) * Math.exp(-t / 0.03)) * Math.exp(-t / decay),
    { dest: fx, gain: g, pan, hall: h, echo: e, fadeIn: 0.001, fadeOut: 0.05 },
  );
}

function glass(t0, f, g, o = {}) {
  const { pan = 0, h = 0.15, e = 0.12, d = 0.05 } = o;
  const parts = [
    [1, 1, d],
    [2.76, 0.5, d * 0.5],
    [5.4, 0.25, d * 0.25],
  ];
  voice(
    t0,
    d * 6,
    (t) => {
      let y = 0;
      for (const [m, a, dd] of parts) y += Math.sin(TAU * f * m * t) * a * Math.exp(-t / dd);
      return y;
    },
    { dest: fx, gain: g, pan, hall: h, echo: e, fadeIn: 0.0006, fadeOut: 0.02 },
  );
  click(t0, g * 0.35, 7000, pan);
}

function blip(t0, f, g, o = {}) {
  const { pan = 0, h = 0.15, e = 0.15, d = 0.04, up = 0 } = o;
  let ph = 0;
  voice(
    t0,
    d * 6,
    (t) => {
      const fr = up ? f * Math.pow(2, (up / 12) * Math.min(1, t / 0.016)) : f * (1 + 0.12 * Math.exp(-t / 0.008));
      ph += (TAU * fr) / SR;
      return (Math.sin(ph) + (up ? 0.22 * Math.sin(3 * ph) : 0.15 * Math.sin(2 * ph))) * Math.exp(-t / d);
    },
    { dest: fx, gain: g, pan, hall: h, echo: e, fadeIn: 0.0008, fadeOut: 0.02 },
  );
}

function heartbeat(t0, g) {
  for (const [dt, a] of [
    [0, 1],
    [0.15, 0.62],
  ]) {
    let ph = 0;
    const f = svf();
    voice(
      t0 + dt,
      0.4,
      (t) => {
        ph += (TAU * (50 + 30 * Math.exp(-t / 0.02))) / SR;
        return f(Math.sin(ph) * Math.exp(-t / 0.075), 220) * a * 1.4;
      },
      { dest: fx, gain: g, hall: 0.12, fadeIn: 0.003, fadeOut: 0.05 },
    );
  }
}

function swell(t0, t1, g) {
  const dur = t1 - t0;
  for (const side of [-1, 1]) {
    const r = rng(ns(t0) + side * 99);
    const f = svf();
    const f2 = svf();
    voice(
      t0,
      dur,
      (t) => {
        const u = t / dur;
        const x = r() * 2 - 1;
        f(x, 2600, 0.6);
        f2(x, 6500 * Math.pow(300 / 6500, smooth(u)), 1.3);
        return (f.hp * 0.7 * Math.exp((u - 1) * 4.5) + f2.bp * Math.pow(u, 3) * 1.3) * 1.2;
      },
      { dest: fx, gain: g, pan: side * 0.55, hall: 0.08, fadeIn: 0.004, fadeOut: 0.004 },
    );
  }
}

function perc(t0, f, g, pan) {
  let ph = 0;
  voice(
    t0,
    0.2,
    (t) => {
      ph += (TAU * f * (1 + 0.5 * Math.exp(-t / 0.012))) / SR;
      return Math.sin(ph) * Math.exp(-t / 0.05);
    },
    { dest: drums, gain: g, pan, room: 0.25, fadeIn: 0.0005, fadeOut: 0.02 },
  );
  click(t0, g * 0.3, 2500, pan);
}

function pluck(t0, f, g, pan) {
  let p = 0;
  const lp = svf();
  voice(
    t0,
    0.34,
    (t) => {
      const dt = f / SR;
      p += dt;
      if (p >= 1) p -= 1;
      return lp(2 * p - 1 - blep(p, dt), 500 + 3200 * Math.exp(-t / 0.035), 0.9) * Math.exp(-t / 0.11);
    },
    { dest: music, gain: g, pan, hall: 0.2, echo: 0.3, fadeIn: 0.001, fadeOut: 0.03 },
  );
}

function freeverb(inp, { size = 0.84, damp = 0.25, width = 1, gain = 0.015, pre = 0 } = {}) {
  const sc = SR / 44100;
  const combT = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
  const apT = [556, 441, 341, 225];
  const mk = (spread) => ({
    cb: combT.map((d) => ({ buf: new Float64Array(Math.round((d + spread) * sc)), i: 0, fs: 0 })),
    ap: apT.map((d) => ({ buf: new Float64Array(Math.round((d + spread) * sc)), i: 0 })),
  });
  const chans = [mk(0), mk(23)];
  const fb = size * 0.28 + 0.7;
  const d1 = damp * 0.4;
  const d2 = 1 - d1;
  const P = ns(pre);
  const out = bus();
  for (let n = 0; n < N; n++) {
    const m = n - P;
    const x = m >= 0 ? (inp.L[m] + inp.R[m]) * gain : 0;
    for (let c = 0; c < 2; c++) {
      const ch = chans[c];
      let y = 0;
      for (let j = 0; j < 8; j++) {
        const cb = ch.cb[j];
        const o = cb.buf[cb.i];
        cb.fs = o * d2 + cb.fs * d1;
        cb.buf[cb.i] = x + cb.fs * fb;
        if (++cb.i >= cb.buf.length) cb.i = 0;
        y += o;
      }
      for (let j = 0; j < 4; j++) {
        const ap = ch.ap[j];
        const o = ap.buf[ap.i];
        ap.buf[ap.i] = y + o * 0.5;
        if (++ap.i >= ap.buf.length) ap.i = 0;
        y = o - y;
      }
      (c ? out.R : out.L)[n] = y;
    }
  }
  const w1 = width / 2 + 0.5;
  const w2 = (1 - width) / 2;
  for (let n = 0; n < N; n++) {
    const l = out.L[n];
    const r = out.R[n];
    out.L[n] = l * w1 + r * w2;
    out.R[n] = r * w1 + l * w2;
  }
  return out;
}

function pingpong(inp, { time = 0.75 * B, fb = 0.38, tone = 0.3 } = {}) {
  const D = ns(time);
  const bl = new Float64Array(D);
  const br = new Float64Array(D);
  let i = 0;
  let zl = 0;
  let zr = 0;
  const out = bus();
  for (let n = 0; n < N; n++) {
    const ol = bl[i];
    const or = br[i];
    zl += (ol - zl) * tone;
    zr += (or - zr) * tone;
    bl[i] = (inp.L[n] + inp.R[n]) * 0.5 + zr * fb;
    br[i] = zl * fb;
    out.L[n] = ol;
    out.R[n] = or;
    if (++i >= D) i = 0;
  }
  return out;
}

function biquad(x, b0, b1, b2, a1, a2) {
  const y = new Float64Array(x.length);
  let x1 = 0;
  let x2 = 0;
  let y1 = 0;
  let y2 = 0;
  for (let n = 0; n < x.length; n++) {
    const v = b0 * x[n] + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
    x2 = x1;
    x1 = x[n];
    y2 = y1;
    y1 = v;
    y[n] = v;
  }
  return y;
}

function highpass(x, fc) {
  const w = (TAU * fc) / SR;
  const cs = Math.cos(w);
  const al = Math.sin(w) / (2 * Math.SQRT1_2);
  const a0 = 1 + al;
  return biquad(x, (1 + cs) / 2 / a0, -(1 + cs) / a0, (1 + cs) / 2 / a0, (-2 * cs) / a0, (1 - al) / a0);
}

function lufs(L, R) {
  const kw = (x) =>
    biquad(
      biquad(x, 1.53512485958697, -2.69169618940638, 1.19839281085285, -1.69065929318241, 0.73248077421585),
      1,
      -2,
      1,
      -1.99004745483398,
      0.99007225036621,
    );
  const kl = kw(L);
  const kr = kw(R);
  const blk = ns(0.4);
  const hop = ns(0.1);
  const z = [];
  for (let s = 0; s + blk <= N; s += hop) {
    let a = 0;
    for (let n = s; n < s + blk; n++) a += kl[n] * kl[n] + kr[n] * kr[n];
    z.push(a / blk);
  }
  const ld = (v) => -0.691 + 10 * Math.log10(v);
  const abs = z.filter((v) => ld(v) > -70);
  const rel = ld(abs.reduce((a, v) => a + v, 0) / abs.length) - 10;
  const g = abs.filter((v) => ld(v) > rel);
  return ld(g.reduce((a, v) => a + v, 0) / g.length);
}

const TAPS = (() => {
  const sinc = (x) => (x === 0 ? 1 : Math.sin(Math.PI * x) / (Math.PI * x));
  return [0.25, 0.5, 0.75].map((ph) => {
    const h = [];
    for (let k = -7; k <= 8; k++) {
      const x = k - ph;
      h.push(sinc(x) * sinc(x / 8));
    }
    const s = h.reduce((a, v) => a + v, 0);
    return h.map((v) => v / s);
  });
})();

function truePeak(L, R) {
  const tp = new Float64Array(N);
  for (let n = 0; n < N; n++) {
    let m = Math.max(Math.abs(L[n]), Math.abs(R[n]));
    for (const h of TAPS) {
      let a = 0;
      let b = 0;
      for (let k = 0; k < 16; k++) {
        const j = n - 7 + k;
        if (j < 0 || j >= N) continue;
        a += h[k] * L[j];
        b += h[k] * R[j];
      }
      m = Math.max(m, Math.abs(a), Math.abs(b));
    }
    tp[n] = m;
  }
  return tp;
}

function limit(L, R, ceil) {
  const tp = truePeak(L, R);
  const LA = ns(0.002);
  const req = new Float64Array(N);
  for (let n = 0; n < N; n++) req[n] = tp[n] > ceil ? ceil / tp[n] : 1;
  const h = new Float64Array(N);
  const dq = [];
  let head = 0;
  for (let n = N - 1; n >= 0; n--) {
    while (dq.length > head && req[dq[dq.length - 1]] >= req[n]) dq.pop();
    dq.push(n);
    while (dq[head] > n + LA) head++;
    h[n] = req[dq[head]];
  }
  const rel = 1 - Math.exp(-1 / (0.08 * SR));
  let acc = LA;
  let r = 1;
  const outL = new Float64Array(N);
  const outR = new Float64Array(N);
  for (let n = 0; n < N; n++) {
    acc += h[n] - (n >= LA ? h[n - LA] : 1);
    const s = Math.min(1, acc / LA);
    r = Math.min(s, r + (1 - r) * rel);
    outL[n] = L[n] * r;
    outR[n] = R[n] * r;
  }
  return [outL, outR];
}

const pos = (k) => k * B;
const beat = (k) => k * B * S;
const sleepA = beat(24);
const wakeT = beat(26);
const finalT = beat(29);
const groove = [];
for (let k = 4 * S; k < 29 * S; k++) if (k < 24 * S || k >= 26 * S) groove.push(k);

[0.2, 0.26, 0.32, 0.38, 0.44, 0.5, 0.56, 0.62].forEach((t, i) => keyClick(t * S, i));

chord(0, 0.94 * S, [hz(45), hz(52), hz(57)], {
  g: 0.05,
  att: 0.9 * S,
  rel: 0.8 * S,
  cut: (t) => 260 + 700 * smooth(t / (0.94 * S)),
  res: 0.9,
  h: 0.3,
  voices: 4,
  cents: 9,
  seed: 3,
  dest: fx,
});
riser(0.45 * S, beat(2), { g: 0.16, f0: 500, f1: 6500, tone: 0.15, p0: 330, p1: 660, seed: 11 });

click(beat(2), 0.35, 3800);
click(beat(2) + 0.018, 0.18, 2600);
voice(beat(2), 0.08, (t) => (Math.sin(TAU * 190 * t) * 0.7 + Math.sin(TAU * 340 * t) * 0.4) * Math.exp(-t / 0.022), { dest: fx, gain: 0.35, room: 0.2 });
boom(beat(2), 0.55, { f0: 140, f1: 42, tp: 0.05, ta: 0.22, noise: 0.45, nd: 0.12, nf: 3200, h: 0.3 });
whoosh(beat(2) + 0.01 * S, 0.55 * S, { f0: 4000, f1: 500, g: 0.2, p0: 0.3, p1: -0.3, peak: 0.12, seed: 21 });

stab(beat(2.5), [hz(57), hz(60), hz(64)], 0.3);
stab(beat(3), [hz(60), hz(64), hz(69)], 0.32);
stab(beat(3.5), [hz(64), hz(69), hz(72)], 0.34);
if (S === 1) [beat(3), beat(3.25), beat(3.5), beat(3.625), beat(3.75), beat(3.875)].forEach((t, i) => snare(t, 0.09 + i * 0.045, 900 + i * 550));
else [0, 0.25, 0.5, 0.75, 1, 1.125, 1.25, 1.375, 1.5, 1.625, 1.75, 1.875].forEach((x, i) => snare(pos(6 + x), 0.07 + i * 0.024, 800 + i * 330));
riser(beat(3), beat(4), { g: 0.2, f0: 600, f1: 9000, tone: 0.2, p0: 440, p1: 1760, seed: 12 });

crash(beat(4), 0.12);
crash(wakeT, 0.14);

for (const k of groove) kick(pos(k), 0.5);
for (const k of groove) if (k % 4 === 1 || k % 4 === 3) clap(pos(k), 0.5);
for (const k of groove) {
  const open = k >= 26 * S;
  const hv = S === 1 ? 1 : k < 12 ? 0 : k < 16 ? 0.55 : k < 24 ? 0.8 : 1;
  if (hv) hat(pos(k + 0.5), (open ? 0.15 : 0.2) * hv, open ? 0.11 : 0.026, 0.22, 500 + k);
  if (k >= 12 * S) {
    const r = rng(800 + k);
    hat(pos(k + 0.25), 0.05 + r() * 0.03, 0.016, -0.3, 900 + k);
    hat(pos(k + 0.75), 0.07 + r() * 0.035, 0.018, -0.25, 1300 + k);
  }
}

const roots = { 1: hz(33), 2: hz(29), 3: hz(36), 4: hz(31), 5: hz(33), 6: hz(29), 7: hz(31) };
for (const k of groove) {
  const f = roots[Math.floor(k / (4 * S))];
  const last = k % 4 === 3;
  const roll = S === 2 && k >= 24 && k < 40 && k % 2 === 1;
  bassNote(pos(k + 0.5), f, last || roll ? 0.1 : 0.17, 0.62);
  if (last) bassNote(pos(k + 0.75), f, 0.09, 0.5, 0.8);
  else if (roll) bassNote(pos(k + 0.75), f, 0.08, 0.4, 0.9);
  if (k >= 26 * S) bassNote(pos(k + 0.5), f * 2, last ? 0.1 : 0.15, 0.22, 1.4);
}

const chords = [
  [hz(57), hz(60), hz(64), hz(67)],
  [hz(53), hz(57), hz(60), hz(64)],
  [hz(55), hz(60), hz(64), hz(67)],
  [hz(55), hz(59), hz(62), hz(69)],
  [hz(57), hz(60), hz(64), hz(67)],
  [hz(53), hz(57), hz(60), hz(64), hz(65), hz(69)],
];
const baseCut = [1800, 2100, 2400, 2700, 2700, 3600];
const sleepFall = (t) => smooth((t - sleepA) / (0.42 * S));
chords.forEach((notes, i) => {
  const t0 = i === 5 ? wakeT : beat(4 + i * 4);
  const t1 = i === 5 ? finalT : i === 4 ? wakeT : t0 + 4 * B * S;
  const sleepy = i === 4;
  chord(t0 - 0.004, t1, notes, {
    g: i === 5 ? 0.04 : 0.05,
    att: 0.02,
    rel: 0.12,
    cut: (t) => {
      const lfo = 1 + 0.12 * Math.sin(TAU * (0.5 / S) * t);
      const c = baseCut[i] * lfo;
      return sleepy ? c + (230 - c) * sleepFall(t) : c;
    },
    bend: sleepy ? (t) => 1 - 0.022 * sleepFall(t) : () => 1,
    res: 0.85,
    h: 0.24,
    seed: 40 + i,
  });
});

[4.5, 5, 5.4, 5.65, 5.9, 6.15, 6.4, 6.65].forEach((b, i) =>
  glass(beat(b), hz(81 + [0, 2, 3, 5, 7, 8, 10, 12][i]), 0.09, { pan: i % 2 ? 0.25 : -0.25 }),
);
glass(beat(7), hz(93), 0.12, { d: 0.08 });
voice(beat(7), 0.12, (t) => Math.sin(TAU * (120 + 60 * Math.exp(-t / 0.01)) * t) * Math.exp(-t / 0.03), { dest: fx, gain: 0.32, hall: 0.15 });
click(beat(7), 0.3, 4200);

whoosh(beat(8), 0.6 * S, { f0: 500, f1: 7000, g: 0.16, p0: -0.5, p1: 0.5, peak: 0.25, seed: 31 });
{
  const r = rng(77);
  const pent = [81, 84, 86, 88, 91, 93, 96];
  for (let i = 0; i < 8; i++) ping(beat(8) + 0.02 * S + r() * 0.38 * S, hz(pent[Math.floor(r() * pent.length)]), 0.05 + r() * 0.03, { pan: (r() - 0.5) * 1.4 });
}

for (let i = 0; i < 9; i++) blip(beat(10) + i * 0.03 * S, hz([76, 79, 81, 84, 79, 81, 84, 86, 88][i]), 0.045, { pan: ((i % 3) - 1) * 0.5 });
[81, 84, 88, 91, 93].forEach((m, i) => bell(beat(11) + i * 0.05 * S, hz(m), 0.06, { pan: -0.4 + i * 0.2, decay: 0.35 }));

whoosh(beat(12), 0.5 * S, { f0: 400, f1: 3500, g: 0.15, p0: 0.5, p1: -0.5, peak: 0.45, seed: 41 });
whoosh(beat(13), 0.28 * S, { f0: 2500, f1: 700, g: 0.13, p0: -0.6, p1: 0.6, peak: 0.3, seed: 42 });
whoosh(beat(13.5), 0.28 * S, { f0: 2500, f1: 700, g: 0.13, p0: 0.6, p1: -0.6, peak: 0.3, seed: 43 });
whoosh(beat(14) - 0.12 * S, 0.5 * S, { f0: 300, f1: 5000, g: 0.17, p0: 0, p1: 0, peak: 0.3, seed: 44 });
[14.75, 15.5].forEach((b, i) => {
  bell(beat(b), hz(i ? 88 : 84), 0.085, { decay: 0.6, ratio: 2.0, index: 1.2, pan: i ? 0.2 : -0.2, h: 0.4, e: 0.25 });
  click(beat(b), 0.28, 3600 + i * 600);
  heartbeat(beat(b), 0.2);
});
voice(beat(16.15), 0.5, (t) => { const f = 150 * Math.exp(-t / 0.12) + 70; return Math.tanh(2 * Math.sin(TAU * f * t)) * Math.exp(-t / 0.18) * 0.9; }, { dest: fx, gain: 0.3, hall: 0.2 });
blip(beat(16.15), hz(70), 0.07, { d: 0.09, pan: 0.3, e: 0.2 });
click(beat(16.15), 0.3, 1800);
[81, 84, 88, 91, 96].forEach((m, i) => bell(beat(16.4) + i * 0.035 * S, hz(m), 0.06, { decay: 0.7, ratio: 2.0, index: 1.0, pan: -0.4 + i * 0.2, h: 0.5, e: 0.25 }));
boom(beat(16.4), 0.35, { f0: 110, f1: 44, tp: 0.06, ta: 0.3, noise: 0.25, nd: 0.15, nf: 2200, h: 0.3 });
whoosh(beat(16.75) - 0.1 * S, 0.4 * S, { f0: 600, f1: 4500, g: 0.15, p0: -0.3, p1: 0.5, peak: 0.35, seed: 45 });

riser(beat(17.3), beat(18.3), { g: 0.17, f0: 500, f1: 8000, tone: 0.2, p0: 330, p1: 1320, seed: 46 });
[7.97, 8.04, 8.11, 8.18, 8.25, 8.32].forEach((t, i) => glass(t * S, hz(86 + (i % 2) * 3), 0.035, { pan: -0.2 + i * 0.08, d: 0.025, e: 0.05 }));
boom(beat(18.3), 0.4, { f0: 130, f1: 40, tp: 0.07, ta: 0.4, noise: 0.3, nd: 0.2, nf: 2800, h: 0.4 });
[84, 88, 91, 96, 100].forEach((m, i) => bell(beat(18.3) + i * 0.03 * S, hz(m), 0.06, { decay: 0.8, ratio: 2.0, index: 1.2, pan: -0.5 + i * 0.25, h: 0.5, e: 0.3 }));
[[18.4, 88], [18.65, 91], [18.85, 86]].forEach(([b, m], i) => { click(beat(b), 0.26, 4200); blip(beat(b), hz(m), 0.05, { up: 3, d: 0.03, pan: i % 2 ? 0.4 : -0.4, e: 0.2 }); });
whoosh(beat(19) - 0.05 * S, 0.5 * S, { f0: 300, f1: 4800, g: 0.16, p0: -0.5, p1: 0.5, peak: 0.4, seed: 47 });
click(beat(19.75), 0.3, 3400);
click(beat(19.75) + 0.012, 0.12, 2400);
bell(beat(19.75), hz(84), 0.085, { decay: 0.6, ratio: 2.0, index: 1.2, pan: -0.15, h: 0.4, e: 0.25 });
bell(beat(19.75) + 0.07 * S, hz(88), 0.08, { decay: 0.7, ratio: 2.0, index: 1.2, pan: 0.15, h: 0.4, e: 0.25 });

riser(beat(20.5), beat(21.3), { g: 0.16, f0: 300, f1: 6000, tone: 0.3, p0: 220, p1: 880, seed: 48 });
[81, 84, 86].forEach((m, i) => bell(beat(20.05 + i * 0.15), hz(m), 0.05, { decay: 0.4, pan: i * 0.3 - 0.3 }));
stab(beat(21.3), [hz(57), hz(64), hz(69)], 0.28);
boom(beat(21.3), 0.4, { f0: 120, f1: 40, tp: 0.07, ta: 0.35, noise: 0.3, nd: 0.2, nf: 2600, h: 0.4 });
whoosh(beat(22) - 0.05 * S, 0.55 * S, { f0: 350, f1: 4200, g: 0.15, p0: -0.4, p1: 0.4, peak: 0.4, seed: 51 });
[10.64, 10.79, 10.94, 11.09].forEach((t, i) => blip(t * S, hz(88 + (i % 2) * 5), 0.04, { up: 2, d: 0.03, pan: i % 2 ? 0.45 : -0.45, e: 0.25 }));

if (S === 1) {
  heartbeat(beat(24), 0.36);
  heartbeat(beat(25), 0.32);
} else [0.36, 0.33, 0.3].forEach((g, i) => heartbeat(pos(48 + i), g));
riser(beat(25), wakeT, { g: 0.2, f0: 300, f1: 8000, tone: 0.3, p0: 110, p1: 880, seed: 61 });

[81, 84, 86, 88, 91, 93].forEach((m, i) => {
  const t = beat(26 + i * 0.5);
  whoosh(t - 0.03 * S, 0.2 * S, { f0: 3000, f1: 700, g: 0.1, p0: i % 2 ? 0.6 : -0.6, p1: i % 2 ? -0.6 : 0.6, peak: 0.2, seed: 70 + i });
  blip(t, hz(m), 0.055, { up: 2, d: 0.035, pan: ((i % 4) - 1.5) * 0.4, e: 0.3 });
  click(t, 0.2, 3000 + i * 300, ((i % 4) - 1.5) * 0.3);
  if (i % 2 === 1) bell(t + 0.004, hz(m - 12), 0.04, { decay: 0.3, pan: ((i % 4) - 1.5) * 0.3 });
});
swell(beat(28), finalT, 0.16);
riser(beat(28), finalT, { g: 0.14, f0: 800, f1: 9000, tone: 0.25, p0: 440, p1: 1760, seed: 71 });

boom(finalT, 0.75, { f0: 120, f1: 38, tp: 0.08, ta: 0.5, noise: 0.5, nd: 0.28, nf: 2600, h: 0.45 });
crash(finalT, 0.1);
chord(finalT, 14.4 * S, [hz(48), hz(55), hz(60), hz(62), hz(64), hz(67)], {
  g: 0.03,
  att: 0.012,
  rel: 0.9 * S,
  cut: (t) => 1600 + 3400 * Math.exp(-(t - finalT) / (0.35 * S)),
  res: 0.8,
  h: 0.5,
  voices: 6,
  cents: 15,
  seed: 90,
  dest: fx,
});
[hz(72), hz(76), hz(79), hz(86)].forEach((f, i) => bell(finalT + 0.01 + i * 0.012, f, 0.05, { decay: 0.9, ratio: 2.0, index: 1.0, pan: -0.45 + i * 0.3, h: 0.5, e: 0.2 }));
voice(
  finalT,
  1.8 * S,
  (t) => {
    let y = 0;
    [hz(84), hz(88), hz(91)].forEach((f, i) => {
      y += Math.sin(TAU * f * t + 0.4 * Math.sin(TAU * (5 + i) * t)) * (0.5 + 0.5 * Math.sin(TAU * (3.1 + i * 0.7) * t + i));
    });
    return (y / 3) * Math.exp(-t / (0.7 * S)) * smooth(t / 0.06);
  },
  { dest: fx, gain: 0.05, hall: 0.6, echo: 0.2, fadeOut: 0.3 * S },
);
[13.81, 13.91, 14.01].forEach((t, i) => ping(t * S, hz([91, 96, 100][i]), 0.05, { pan: [0, 0.5, -0.5][i], decay: 0.09 }));
for (let i = 0; i < 8; i++) glass(13.93 * S + i * 0.04 * S, hz(96 + (i % 3) * 2), 0.022, { pan: -0.5 + i / 7, d: 0.02, e: 0.05 });

if (S === 2) {
  crash(pos(24), 0.07);
  crash(pos(40), 0.08);
  for (let k = 16; k < 24; k++) {
    const r = rng(2000 + k);
    for (let j = 0; j < 4; j++) hat(pos(k + j * 0.25), (j % 2 ? 0.05 : 0.032) * (0.5 + (0.5 * (k - 16)) / 7) * (0.85 + r() * 0.3), 0.011, 0.38, 2100 + k * 4 + j);
  }
  for (let bar = 8; bar < 14; bar++) {
    if (bar === 11) continue;
    [0.75, 1.5, 2.75, 3.5].forEach((x, i) => perc(pos(bar * 4 + x), i % 2 ? 290 : 385, bar >= 12 ? 0.17 : 0.13, i % 2 ? 0.4 : -0.4));
  }
  const steps = [0, 2, 1, 3, 2, 0, 3, 1];
  for (let k = 28; k < 58; k++) {
    if (k >= 48 && k < 52) continue;
    const notes = chords[Math.min(5, Math.floor(k / 8) - 1)];
    for (let h = 0; h < 2; h++) {
      const n = (k % 4) * 2 + h;
      pluck(pos(k + h * 0.5), notes[steps[n] % notes.length], (k >= 52 ? 0.085 : k >= 32 ? 0.07 : 0.055) * (h ? 0.8 : 1), ((n % 4) - 1.5) * 0.3);
    }
  }
  for (let j = 0; j < 8; j++) snare(pos(51 + j * 0.125), 0.06 + j * 0.035, 1500 + j * 480);
}

const kicks = groove.map(pos);
const duck = new Float64Array(N).fill(1);
for (const tk of kicks) {
  const n0 = ns(tk);
  const pre = ns(0.003);
  const len = ns(0.27);
  for (let i = -pre; i < len; i++) {
    const n = n0 + i;
    if (n < 0 || n >= N) continue;
    const d = i < 0 ? 0.58 * (1 + i / pre) : 0.58 * Math.pow(1 - i / len, 2);
    duck[n] = Math.min(duck[n], 1 - d);
  }
}

const wetHall = freeverb(hall, { size: 0.9, damp: 0.35, width: 1, gain: 0.015, pre: 0.025 });
const wetRoom = freeverb(room, { size: 0.55, damp: 0.5, width: 0.8, gain: 0.015, pre: 0.008 });
const wetEcho = pingpong(echo, { time: 0.75 * B, fb: 0.4, tone: 0.28 });

let L = new Float64Array(N);
let R = new Float64Array(N);
const fadeA = 14.2 * S;
const fadeB = 14.95 * S;
for (let n = 0; n < N; n++) {
  const dl = drums.L[n];
  const dr = drums.R[n];
  const t = n / SR;
  const fade = t < fadeA ? 1 : t >= fadeB ? 0 : Math.pow(Math.cos(((t - fadeA) / (fadeB - fadeA)) * (Math.PI / 2)), 2);
  L[n] = (Math.tanh(dl * 1.15) / 1.15 + music.L[n] * duck[n] + fx.L[n] + wetHall.L[n] * 0.9 + wetRoom.L[n] * 0.8 + wetEcho.L[n] * 0.55) * fade;
  R[n] = (Math.tanh(dr * 1.15) / 1.15 + music.R[n] * duck[n] + fx.R[n] + wetHall.R[n] * 0.9 + wetRoom.R[n] * 0.8 + wetEcho.R[n] * 0.55) * fade;
}
L = highpass(L, 24);
R = highpass(R, 24);
for (let n = ns(fadeB); n < N; n++) {
  L[n] = 0;
  R[n] = 0;
}

let gain = Math.pow(10, (TARGET_LUFS - lufs(L, R)) / 20);
let fL;
let fR;
let measured = 0;
for (let it = 0; it < 4; it++) {
  [fL, fR] = limit(
    L.map((v) => v * gain),
    R.map((v) => v * gain),
    CEILING,
  );
  measured = lufs(fL, fR);
  if (Math.abs(measured - TARGET_LUFS) < 0.05) break;
  gain *= Math.pow(10, (TARGET_LUFS - measured) / 20);
}

const buf = Buffer.alloc(44 + N * 4);
buf.write('RIFF', 0);
buf.writeUInt32LE(36 + N * 4, 4);
buf.write('WAVE', 8);
buf.write('fmt ', 12);
buf.writeUInt32LE(16, 16);
buf.writeUInt16LE(1, 20);
buf.writeUInt16LE(2, 22);
buf.writeUInt32LE(SR, 24);
buf.writeUInt32LE(SR * 4, 28);
buf.writeUInt16LE(4, 32);
buf.writeUInt16LE(16, 34);
buf.write('data', 36);
buf.writeUInt32LE(N * 4, 40);
for (let n = 0; n < N; n++) {
  buf.writeInt16LE(Math.round(clamp(fL[n], -1, 1) * 32767), 44 + n * 4);
  buf.writeInt16LE(Math.round(clamp(fR[n], -1, 1) * 32767), 46 + n * 4);
}
mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, buf);
console.log(`${OUT} ${N} frames, ${measured.toFixed(2)} LUFS, gain ${(20 * Math.log10(gain)).toFixed(2)} dB`);
