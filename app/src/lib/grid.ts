/** A steady beat grid: tempo and the time of one bar one (beat 1). */
export interface Grid {
  bpm: number;
  anchorMs: number;
}

export const periodMs = (g: Grid) => 60_000 / g.bpm;

/** Every beat up to `durationMs`: [time ms, beat in bar 1..4], as the backend lays them. */
export function gridBeats(g: Grid, durationMs: number): [number, number][] {
  if (!(g.bpm > 0)) return [];
  const p = periodMs(g);
  const out: [number, number][] = [];
  for (let k = Math.ceil(-g.anchorMs / p); ; k++) {
    const t = g.anchorMs + k * p;
    if (t > durationMs) break;
    if (t >= 0) out.push([Math.round(t), (((k % 4) + 4) % 4) + 1]);
  }
  return out;
}

/** Index (from the anchor) of the beat nearest to `ms`. */
export const nearestBeat = (g: Grid, ms: number) => Math.round((ms - g.anchorMs) / periodMs(g));

/** A new tempo, keeping the beat nearest to `pivotMs` where it is. */
export function withTempo(g: Grid, bpm: number, pivotMs: number): Grid {
  const k = nearestBeat(g, pivotMs);
  const pivot = g.anchorMs + k * periodMs(g);
  return { bpm, anchorMs: pivot - k * (60_000 / bpm) };
}

/** The anchor moved by whole bars to the first bar one at or after 0. */
export function normalize(g: Grid): Grid {
  const bar = 4 * periodMs(g);
  return { bpm: g.bpm, anchorMs: g.anchorMs - Math.floor(g.anchorMs / bar) * bar };
}
