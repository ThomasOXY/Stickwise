import type { Waveform } from "./api";

/** One column ready to draw: stacked band heights (0..1) or a single coloured bar. */
type Column = { low: number; mid: number; high: number } | { height: number; color: string };

const LOW = "rgb(32 83 217)";
const MID = "rgb(242 170 60)";
const HIGH = "rgb(255 255 255)";

/** Columns per second of the scrolling (detail) waveforms. */
export const DETAIL_RATE = 150;

export function columnCount(w: Waveform): number {
  return Math.floor(w.data.length / w.entryLen);
}

/** Decode column `i`. Heights are not normalised yet. */
function column(w: Waveform, i: number): Column {
  const d = w.data;
  if (w.kind === "three-band") {
    // mid, high, low per column; weights as the CDJ-3000 stacks them.
    const o = i * 3;
    return { low: d[o + 2] * 0.49, mid: d[o] * 0.32, high: d[o + 1] * 0.25 };
  }
  if (w.kind === "color") {
    const word = (d[i * 2] << 8) | d[i * 2 + 1];
    const r = ((word >> 13) & 7) * 36;
    const g = ((word >> 10) & 7) * 36;
    const b = ((word >> 7) & 7) * 36;
    return { height: ((word >> 2) & 31) / 31, color: `rgb(${r} ${g} ${b})` };
  }
  const v = d[i];
  const white = (v >> 5) / 7;
  const mix = (a: number, b: number) => Math.round(a + (b - a) * white);
  return { height: (v & 31) / 31, color: `rgb(${mix(43, 116)} ${mix(89, 246)} ${mix(255, 244)})` };
}

function peak(w: Waveform): number {
  if (w.kind !== "three-band") return 1;
  let max = 1;
  for (let i = 0; i < columnCount(w); i++) {
    const c = column(w, i) as { low: number; mid: number; high: number };
    max = Math.max(max, c.low + c.mid + c.high);
  }
  return max;
}

/**
 * Draw columns `[from, to)` of `w` into the canvas, mirrored around the
 * centre line. Several columns per pixel are merged by taking the loudest.
 */
export function draw(canvas: HTMLCanvasElement, w: Waveform, from = 0, to = columnCount(w), dim = 1) {
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, width, height);
  ctx.globalAlpha = dim;
  const mid = height / 2;
  const half = height / 2 - 1;
  const norm = peak(w);
  const span = Math.max(1, to - from);
  const step = 1 / dpr;

  for (let x = 0; x < width; x += step) {
    const a = from + Math.floor((x / width) * span);
    const b = Math.max(a + 1, from + Math.floor(((x + step) / width) * span));
    let best: Column | null = null;
    let bestH = -1;
    for (let i = a; i < b && i < to; i++) {
      const c = column(w, i);
      const h = "low" in c ? c.low + c.mid + c.high : c.height;
      if (h > bestH) {
        bestH = h;
        best = c;
      }
    }
    if (!best) continue;
    if ("low" in best) {
      const bands: [number, string][] = [
        [(best.low + best.mid + best.high) / norm, HIGH],
        [(best.low + best.mid) / norm, MID],
        [best.low / norm, LOW],
      ];
      for (const [h, color] of bands) {
        const px = h * half;
        ctx.fillStyle = color;
        ctx.fillRect(x, mid - px, step, px * 2);
      }
    } else {
      const px = best.height * half;
      ctx.fillStyle = best.color;
      ctx.fillRect(x, mid - px, step, px * 2);
    }
  }
}
