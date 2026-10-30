import type { Waveform } from "./api";

/** rekordbox waveform styles, named as in its preferences. */
export type Style = "three-band" | "color" | "mono";

export const STYLE_LABELS: Record<Style, string> = { "three-band": "3Band", color: "RGB", mono: "Blue" };

/** Columns per second of the scrolling (detail) waveforms. */
export const DETAIL_RATE = 150;

export function columnCount(w: Waveform): number {
  return Math.floor(w.data.length / w.entryLen);
}

// CDJ-3000 / rekordbox 3Band colours (as used by beat-link).
const LOW: Rgb = [32, 83, 217];
const MID: Rgb = [242, 170, 60];
const HIGH: Rgb = [255, 255, 255];
// rekordbox "Blue" waveform: dark blue body, light cyan where it is bright.
const BLUE: Rgb = [43, 89, 255];
const BLUE_INTENSE: Rgb = [116, 246, 244];

type Rgb = [number, number, number];

/** A sample normalised to 0..1: stacked band heights, or one height with a colour. */
interface Sample {
  low: number;
  mid: number;
  high: number;
  /** Total height (for single-colour styles equal to `low`). */
  h: number;
  color: Rgb;
}

const peaks = new WeakMap<Waveform, number>();

function rawThreeBand(d: number[], i: number) {
  // Byte order is mid, high, low; weights as the CDJ-3000 stacks them.
  const o = i * 3;
  return [d[o + 2] * 0.49, d[o] * 0.32, d[o + 1] * 0.25] as const;
}

function peak(w: Waveform): number {
  let p = peaks.get(w);
  if (p !== undefined) return p;
  p = 1;
  if (w.kind === "three-band") {
    for (let i = 0; i < columnCount(w); i++) {
      const [l, m, h] = rawThreeBand(w.data, i);
      p = Math.max(p, l + m + h);
    }
  }
  peaks.set(w, p);
  return p;
}

function sampleAt(w: Waveform, i: number, out: Sample) {
  const d = w.data;
  if (w.kind === "three-band") {
    const [l, m, h] = rawThreeBand(d, i);
    const p = peak(w);
    out.low = l / p;
    out.mid = (l + m) / p;
    out.high = (l + m + h) / p;
    out.h = out.high;
  } else if (w.kind === "color") {
    const word = (d[i * 2] << 8) | d[i * 2 + 1];
    out.h = out.high = out.mid = out.low = ((word >> 2) & 31) / 31;
    out.color = [((word >> 13) & 7) * 36, ((word >> 10) & 7) * 36, ((word >> 7) & 7) * 36];
  } else {
    const v = d[i];
    const white = (v >> 5) / 7;
    out.h = out.high = out.mid = out.low = (v & 31) / 31;
    out.color = [
      BLUE[0] + (BLUE_INTENSE[0] - BLUE[0]) * white,
      BLUE[1] + (BLUE_INTENSE[1] - BLUE[1]) * white,
      BLUE[2] + (BLUE_INTENSE[2] - BLUE[2]) * white,
    ];
  }
}

const tmpA: Sample = { low: 0, mid: 0, high: 0, h: 0, color: [0, 0, 0] };
const tmpB: Sample = { low: 0, mid: 0, high: 0, h: 0, color: [0, 0, 0] };
const out: Sample = { low: 0, mid: 0, high: 0, h: 0, color: [0, 0, 0] };

/**
 * The sample covering columns [a, b). Zoomed out (several columns per
 * pixel) it takes the loudest column; zoomed in it interpolates between
 * neighbouring columns so the shape stays smooth.
 */
function sampleRange(w: Waveform, a: number, b: number, n: number): Sample | null {
  if (b <= 0 || a >= n) return null;
  if (b - a < 1) {
    const c = Math.max(0, (a + b) / 2 - 0.5);
    const i = Math.min(n - 1, Math.floor(c));
    const j = Math.min(n - 1, i + 1);
    const f = c - i;
    sampleAt(w, i, tmpA);
    sampleAt(w, j, tmpB);
    out.low = tmpA.low + (tmpB.low - tmpA.low) * f;
    out.mid = tmpA.mid + (tmpB.mid - tmpA.mid) * f;
    out.high = tmpA.high + (tmpB.high - tmpA.high) * f;
    out.h = tmpA.h + (tmpB.h - tmpA.h) * f;
    for (let k = 0; k < 3; k++) out.color[k] = tmpA.color[k] + (tmpB.color[k] - tmpA.color[k]) * f;
    return out;
  }
  const from = Math.max(0, Math.floor(a));
  const to = Math.min(n, Math.ceil(b));
  out.h = -1;
  for (let i = from; i < to; i++) {
    sampleAt(w, i, tmpA);
    if (tmpA.h > out.h) {
      out.low = tmpA.low;
      out.mid = tmpA.mid;
      out.high = tmpA.high;
      out.h = tmpA.h;
      out.color[0] = tmpA.color[0];
      out.color[1] = tmpA.color[1];
      out.color[2] = tmpA.color[2];
    }
  }
  return out.h < 0 ? null : out;
}

function pack(c: Rgb, alpha = 255): number {
  return ((alpha << 24) | (Math.round(c[2]) << 16) | (Math.round(c[1]) << 8) | Math.round(c[0])) >>> 0;
}

/** Size the canvas backing store to its CSS size; returns device-pixel size. */
export function fit(canvas: HTMLCanvasElement): { w: number; h: number; dpr: number } {
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
  if (canvas.width !== w) canvas.width = w;
  if (canvas.height !== h) canvas.height = h;
  return { w, h, dpr };
}

const images = new WeakMap<HTMLCanvasElement, ImageData>();

/**
 * Paint the time window [t0, t1] seconds of `w` (which has `rate` columns
 * per second) into the whole canvas, mirrored around the centre line.
 */
export function paint(canvas: HTMLCanvasElement, w: Waveform, rate: number, t0: number, t1: number) {
  const { w: W, h: H } = fit(canvas);
  const ctx = canvas.getContext("2d")!;
  // One buffer per canvas, reused every frame: no garbage while scrolling.
  let img = images.get(canvas);
  if (!img || img.width !== W || img.height !== H) {
    img = ctx.createImageData(W, H);
    images.set(canvas, img);
  }
  const px = new Uint32Array(img.data.buffer);
  px.fill(0);
  const n = columnCount(w);
  const mid = H / 2;
  const half = H / 2 - 1;
  const colsPerPx = ((t1 - t0) * rate) / W;
  // Pixel x covers columns [(x0 + x) * colsPerPx, …): a fixed grid, whatever the start.
  const x0 = Math.round((t0 * rate) / colsPerPx);
  const three = w.kind === "three-band";
  const cLow = pack(LOW);
  const cMid = pack(MID);
  const cHigh = pack(HIGH);

  for (let x = 0; x < W; x++) {
    const a = (x0 + x) * colsPerPx;
    const s = sampleRange(w, a, a + colsPerPx, n);
    if (!s) continue;
    const top = Math.round(mid - s.h * half);
    const bottom = Math.round(mid + s.h * half);
    if (three) {
      const yMid = s.mid * half;
      const yLow = s.low * half;
      for (let y = top; y < bottom; y++) {
        const d = Math.abs(y + 0.5 - mid);
        px[y * W + x] = d <= yLow ? cLow : d <= yMid ? cMid : cHigh;
      }
    } else {
      const c = pack(s.color);
      for (let y = top; y < bottom; y++) px[y * W + x] = c;
    }
  }
  ctx.putImageData(img, 0, 0);
  return ctx;
}
