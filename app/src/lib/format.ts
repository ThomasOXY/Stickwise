import { i18n } from "./i18n.svelte";

export function time(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

export function cueTime(ms: number): string {
  const m = Math.floor(ms / 60000);
  const s = ((ms % 60000) / 1000).toFixed(1).padStart(4, "0");
  return `${m}:${s}`;
}

export function gb(bytes: number): string {
  return (bytes / 1e9).toFixed(1);
}

export function size(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.round(bytes / 1e3)} kB`;
}

const CLASSIC: Record<string, string> = {
  Abm: "1A", B: "1B", Ebm: "2A", "F#": "2B", Gb: "2B", Bbm: "3A", Db: "3B", Fm: "4A", Ab: "4B",
  Cm: "5A", Eb: "5B", Gm: "6A", Bb: "6B", Dm: "7A", F: "7B", Am: "8A", C: "8B", Em: "9A", G: "9B",
  Bm: "10A", D: "10B", "F#m": "11A", Gbm: "11A", A: "11B", "C#m": "12A", Dbm: "12A", E: "12B",
  "G#m": "1A",
};

/** Position on the Camelot wheel (1A, 1B, 2A, …) for sorting; unknown keys last. */
export function keyOrder(key: string): number {
  const camelot = /^\d{1,2}[AB]$/.test(key) ? key : CLASSIC[key];
  return camelot ? parseInt(camelot, 10) * 2 + (camelot.endsWith("B") ? 1 : 0) : Infinity;
}

/** "2024-05-01" in the app's language: "1. 5. 2024", "May 1, 2024", "1.5.2024". */
export function date(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return "";
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return d.toLocaleDateString(i18n.lang, { day: "numeric", month: i18n.lang === "en" ? "short" : "numeric", year: "numeric" });
}

/** A count with the app language's thousands separator. */
export function num(n: number): string {
  return n.toLocaleString(i18n.lang);
}

/** Sorting text the way the app's language does. */
export function collator(): Intl.Collator {
  return new Intl.Collator(i18n.lang, { numeric: true, sensitivity: "base" });
}

/** Swatch colour for a key on the Camelot wheel. */
export function keyColor(key: string): string {
  const camelot = /^\d{1,2}[AB]$/.test(key) ? key : CLASSIC[key];
  if (!camelot) return "var(--text-3)";
  const n = parseInt(camelot, 10);
  const hue = (165 + (n - 1) * 30) % 360;
  return `hsl(${hue} 68% ${camelot.endsWith("A") ? 58 : 66}%)`;
}

export const HOT_CUE_VARS = ["--hc-a", "--hc-b", "--hc-c", "--hc-d", "--hc-e", "--hc-f", "--hc-g", "--hc-h"];

export function rgb(c: [number, number, number] | null | undefined, slot = 0): string {
  if (c && (c[0] || c[1] || c[2])) return `rgb(${c[0]} ${c[1]} ${c[2]})`;
  return slot >= 1 && slot <= 8 ? `var(${HOT_CUE_VARS[slot - 1]})` : "var(--text-2)";
}

export function fsName(fs: string): string {
  return ({ msdos: "FAT32", exfat: "exFAT", hfs: "HFS+", apfs: "APFS" } as Record<string, string>)[fs] ?? fs;
}
