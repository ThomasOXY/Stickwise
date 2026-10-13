import { listen } from "@tauri-apps/api/event";
import {
  api,
  type CueSummary,
  type JobEvent,
  type Playlist,
  type StickData,
  type StickInfo,
  type Track,
  type TrackDetail,
  type VerifyResult,
} from "./api";

export type Selection =
  | { kind: "playlist"; id: number }
  | { kind: "history"; id: number }
  | { kind: "player"; player: string };

export type View = "tracks" | "health";

class AppState {
  sticks = $state<StickInfo[]>([]);
  stickPath = $state<string | null>(null);
  data = $state<StickData | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  selection = $state<Selection | null>(null);
  view = $state<View>("tracks");
  playlistFilter = $state("");
  trackFilter = $state("");

  verify = $state<VerifyResult | null>(null);
  verifying = $state(false);
  verifyProgress = $state<[number, number]>([0, 0]);

  cues = $state<Record<number, CueSummary["hot"]>>({});
  selectedTrack = $state<number | null>(null);
  detailOpen = $state(false);
  detail = $state<TrackDetail | null>(null);

  playing = $state<number | null>(null);
  jobs = $state<JobEvent[]>([]);
  exportOpen = $state(false);

  trackById = $derived(new Map((this.data?.tracks ?? []).map((t) => [t.id, t])));

  /** Track id → "error" | "warning" from the last health check. */
  status = $derived.by(() => {
    const m = new Map<number, "error" | "warning">();
    for (const i of this.verify?.issues ?? []) {
      if (i.severity === "error" || !m.has(i.trackId)) m.set(i.trackId, i.severity);
    }
    return m;
  });

  formatOverride = $derived.by(() => {
    const m = new Map<number, string>();
    for (const i of this.verify?.issues ?? []) {
      if (i.kind === "float") m.set(i.trackId, "WAV 32f");
      if (i.kind === "broken-tag") m.set(i.trackId, "MP3 tag");
    }
    return m;
  });

  errorTracks = $derived(new Set((this.verify?.issues ?? []).filter((i) => i.severity === "error").map((i) => i.trackId)));

  currentTrackIds = $derived.by((): number[] => {
    const s = this.selection;
    const d = this.data;
    if (!s || !d) return [];
    if (s.kind === "playlist") return d.playlists.find((p) => p.id === s.id)?.trackIds ?? [];
    if (s.kind === "history") return d.history.find((p) => p.id === s.id)?.trackIds ?? [];
    return this.verify?.players.find((p) => p.player === s.player)?.unplayable ?? [];
  });

  currentTracks = $derived.by((): Track[] => {
    const q = this.trackFilter.trim().toLowerCase();
    return this.currentTrackIds
      .map((id) => this.trackById.get(id))
      .filter((t): t is Track => !!t)
      .filter((t) => !q || `${t.title} ${t.artist}`.toLowerCase().includes(q));
  });

  currentTitle = $derived.by(() => {
    const s = this.selection;
    if (!s || !this.data) return "";
    if (s.kind === "playlist") return this.data.playlists.find((p) => p.id === s.id)?.name ?? "";
    if (s.kind === "history") return this.data.history.find((p) => p.id === s.id)?.name ?? "";
    return `Nepůjde na ${s.player}`;
  });

  async init() {
    await listen<[number, number]>("verify-progress", (e) => (this.verifyProgress = e.payload));
    await listen<JobEvent>("job", (e) => {
      const i = this.jobs.findIndex((j) => j.id === e.payload.id);
      if (i >= 0) this.jobs[i] = e.payload;
      else this.jobs.push(e.payload);
    });
    await this.refreshSticks();
    setInterval(() => this.refreshSticks(), 2500);
  }

  async refreshSticks() {
    try {
      const list = await api.listSticks();
      this.sticks = list;
      if (this.stickPath && !list.some((s) => s.path === this.stickPath)) {
        this.close();
      }
      if (!this.stickPath && list.length > 0) await this.open(list[0].path);
    } catch (e) {
      this.error = String(e);
    }
  }

  close() {
    this.stickPath = null;
    this.data = null;
    this.verify = null;
    this.selection = null;
    this.cues = {};
    this.detail = null;
    this.detailOpen = false;
    this.selectedTrack = null;
  }

  async open(path: string) {
    if (this.stickPath === path && this.data) return;
    this.close();
    this.stickPath = path;
    this.loading = true;
    this.error = null;
    try {
      this.data = await api.openStick(path);
      const first = this.sortedChildren(0).find((p) => !p.isFolder) ?? this.data.playlists.find((p) => !p.isFolder);
      if (first) this.select({ kind: "playlist", id: first.id });
      this.runVerify();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  sortedChildren(parent: number): Playlist[] {
    return (this.data?.playlists ?? []).filter((p) => p.parentId === parent).sort((a, b) => a.sortOrder - b.sortOrder);
  }

  select(s: Selection) {
    this.selection = s;
    this.view = "tracks";
    this.trackFilter = "";
    this.loadCues(this.currentTrackIds);
  }

  async loadCues(ids: number[]) {
    const path = this.stickPath;
    const missing = ids.filter((id) => !(id in this.cues));
    if (!path || missing.length === 0) return;
    try {
      for (const c of await api.cueSummary(path, missing)) this.cues[c.trackId] = c.hot;
    } catch {
      /* the table simply shows no cue squares */
    }
  }

  async runVerify() {
    const path = this.stickPath;
    if (!path || this.verifying) return;
    this.verifying = true;
    this.verifyProgress = [0, this.data?.tracks.length ?? 0];
    try {
      const r = await api.verifyStick(path);
      if (this.stickPath === path) this.verify = r;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.verifying = false;
    }
  }

  async selectTrack(id: number) {
    this.selectedTrack = id;
    const path = this.stickPath;
    if (!path) return;
    if (this.detail?.trackId === id) return;
    try {
      const d = await api.trackDetail(path, id);
      if (this.selectedTrack === id) this.detail = d;
    } catch (e) {
      this.error = String(e);
    }
  }
}

export const app = new AppState();
