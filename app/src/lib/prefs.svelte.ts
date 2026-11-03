import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * The app's settings, edited in the Settings window. The sheets start from
 * them; what is changed in a sheet holds for that once.
 */
export interface Prefs {
  stick: {
    /** All playlists at the top of the stick, no folders. */
    flat: boolean;
    /** Write My Settings to the stick on sync and format. */
    writeSettings: boolean;
    /** Engine DJ schema for a hybrid (Denon) stick, `null` = Pioneer only. A stick keeps its own. */
    engine: string | null;
  };
  planner: {
    minutes: number;
    level: string;
    curve: string;
    keepOrder: boolean;
    /** `only`, `hot` or `memory`. */
    cueMode: string;
  };
  export: {
    numbered: boolean;
    artistTitle: boolean;
    m3u: boolean;
    history: boolean;
    duplicates: "clone" | "copy" | "symlink";
  };
}

/** Engine DJ library versions for a hybrid stick: the oldest Denon firmware it plays on. */
export const ENGINE: { schema: string; label: string; note: string }[] = [
  { schema: "2.21.0", label: "Engine OS 3.1+", note: "i nejstarší Denony" },
  { schema: "3.0.1", label: "Engine OS 4.3+", note: "doporučeno" },
  { schema: "3.0.2", label: "Engine OS 5+", note: "jen nejnovější" },
];

export const PLAN_LEVELS = [
  ["beginner", "Začátečník", "Jen harmonicky čisté páry, tempo ±3 %, dlouhé 32taktové přechody."],
  ["advanced", "Pokročilý", "Energy boosty (+2), tempo ±6 %, 16taktové přechody."],
  ["pro", "Profík", "Odvážnější tóniny a tempo, krátké 8taktové přechody, střihy."],
] as const;

export const PLAN_CURVES = [
  ["warmup", "Warm-up"],
  ["peak", "Peak"],
  ["journey", "Cesta"],
  ["closing", "Closing"],
] as const;

export const PLAN_CUES = [
  ["only", "Jen přechody", "Na padech zůstanou jen cue přechodů (start, swap, drop, mix out) v pořadí A, B, …; ostatní hot cue se zazálohují a smažou, memory cue zůstanou"],
  ["hot", "Na volné pady", "Tvoje hot cue zůstanou, cue přechodů přibudou na nejvyšší volné pady (H, G, …)"],
  ["memory", "Memory", "Jako memory cue, pady zůstanou, jak jsou"],
] as const;

export const DUPLICATES = [
  ["clone", "Klon (nezabírá místo)"],
  ["copy", "Kopie"],
  ["symlink", "Odkaz"],
] as const;

export const DEFAULTS: Prefs = {
  stick: { flat: true, writeSettings: true, engine: null },
  planner: { minutes: 60, level: "advanced", curve: "peak", keepOrder: true, cueMode: "only" },
  export: { numbered: true, artistTitle: true, m3u: true, history: false, duplicates: "clone" },
};

/** Saved settings over the defaults, so a setting added later has its default. */
function merge(saved: Partial<Prefs> | null): Prefs {
  return {
    stick: { ...DEFAULTS.stick, ...saved?.stick },
    planner: { ...DEFAULTS.planner, ...saved?.planner },
    export: { ...DEFAULTS.export, ...saved?.export },
  };
}

/** What the sheets remembered before there were settings: the starting point. */
function remembered(): Prefs {
  const get = (k: string) => localStorage.getItem(k);
  const p = merge(null);
  if (get("syncFlat")) p.stick.flat = get("syncFlat") !== "off";
  if (get("syncSettings")) p.stick.writeSettings = get("syncSettings") !== "off";
  if (get("plan-minutes")) p.planner.minutes = Number(get("plan-minutes")) || p.planner.minutes;
  p.planner.level = get("plan-level") || p.planner.level;
  p.planner.curve = get("plan-curve") || p.planner.curve;
  if (get("plan-order")) p.planner.keepOrder = get("plan-order") !== "suggest";
  p.planner.cueMode = get("plan-cue-mode") || p.planner.cueMode;
  return p;
}

class PrefsStore {
  value = $state<Prefs>(merge(null));
  loaded = $state(false);

  async init() {
    listen<Partial<Prefs>>("prefs-changed", (e) => (this.value = merge(e.payload)));
    const saved = await invoke<Partial<Prefs> | null>("prefs_get");
    if (saved) {
      this.value = merge(saved);
    } else {
      this.value = remembered();
      await this.save();
    }
    this.loaded = true;
  }

  /** Change one group, e.g. `set("stick", { flat: false })`. */
  set<K extends keyof Prefs>(group: K, change: Partial<Prefs[K]>) {
    this.value = { ...this.value, [group]: { ...this.value[group], ...change } };
    return this.save();
  }

  reset<K extends keyof Prefs>(group: K) {
    this.value = { ...this.value, [group]: { ...DEFAULTS[group] } };
    return this.save();
  }

  private save() {
    return invoke("prefs_set", { prefs: $state.snapshot(this.value) });
  }
}

export const prefs = new PrefsStore();

/** Open the Settings window, at a section (`general`, `player`, `stick`, `planner`, `export`, `peer`). */
export function openSettings(section?: string) {
  return invoke("open_settings", { section: section ?? null });
}
