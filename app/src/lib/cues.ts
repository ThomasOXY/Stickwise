import type { Cue } from "./api";

export const SLOT_LETTERS = ["A", "B", "C", "D", "E", "F", "G", "H"];

/** Snap to the nearest beat of the grid (times in ms). */
export function snap(ms: number, beats: [number, number][]): number {
  if (beats.length === 0) return Math.round(ms);
  let lo = 0;
  let hi = beats.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (beats[mid][0] < ms) lo = mid + 1;
    else hi = mid;
  }
  const a = beats[Math.max(0, lo - 1)][0];
  const b = beats[lo][0];
  return Math.abs(a - ms) <= Math.abs(b - ms) ? a : b;
}

/** Time `n` beats after the beat at (or nearest to) `ms`; falls back to the tempo. */
export function beatsLater(ms: number, n: number, beats: [number, number][], bpm: number): number {
  const i = beats.findIndex(([t]) => t >= ms - 1);
  if (i >= 0 && i + n < beats.length) return beats[i + n][0];
  return Math.round(ms + (n * 60000) / Math.max(1, bpm));
}

export function blankCue(slot: number, timeMs: number): Cue {
  return {
    slot,
    timeMs: Math.max(0, Math.round(timeMs)),
    loopEndMs: null,
    loopBeats: null,
    comment: "",
    hotColor: 0,
    hotLed: null,
    memoryColor: 0,
    activeLoop: false,
    color: null,
  };
}

export function hotCue(cues: Cue[], slot: number): Cue | undefined {
  return cues.find((c) => c.slot === slot);
}

export function firstFreeSlot(cues: Cue[]): number | null {
  for (let s = 1; s <= 8; s++) if (!hotCue(cues, s)) return s;
  return null;
}

export function withoutSlot(cues: Cue[], slot: number): Cue[] {
  return cues.filter((c) => c.slot !== slot);
}

export function replaceCue(cues: Cue[], old: Cue, next: Cue): Cue[] {
  return cues.map((c) => (c === old ? next : c));
}

export function cssColor(c: Cue): string {
  const rgb = c.color;
  return rgb ? `rgb(${rgb[0]} ${rgb[1]} ${rgb[2]})` : "var(--text-2)";
}
