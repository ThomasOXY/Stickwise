import { listen } from "@tauri-apps/api/event";
import {
  api,
  type Cue,
  type CueSummary,
  type PaletteColor,
  type SmartCue,
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

  /** Tracks with cue edits not yet written to the stick. */
  pending = $state<Set<number>>(new Set());
  palette = $state<PaletteColor[]>([]);
  quantize = $state(localStorage.getItem("quantize") !== "off");
  writeOpen = $state(false);
  /** Smart Cue suggestions for the selected track, shown on the waveform. */
  smart = $state<SmartCue[] | null>(null);
  smartOpen = $state(false);
  /** CUE-DETR sidecar installed, and which tracks it has analysed. */
  aiInstalled = $state(false);
  aiAnalyzed = $state<Set<number>>(new Set());
  aiJob = $state<number | null>(null);
  notice = $state<string | null>(null);
  private undoStack: { trackId: number; cues: Cue[] }[] = [];

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
      if (e.payload.id === this.aiJob && e.payload.state !== "running") this.aiFinished(e.payload);
    });
    this.palette = await api.cuePalette();
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
      this.loadPending();
      this.runVerify();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  async loadPending() {
    if (!this.stickPath) return;
    this.pending = new Set(await api.pendingCues(this.stickPath));
  }

  /** Replace the cues of the selected track (kept in the app until written to the stick). */
  async setCues(cues: Cue[], undoable = true) {
    const d = this.detail;
    const path = this.stickPath;
    if (!d || !path) return;
    if (undoable) {
      this.undoStack.push({ trackId: d.trackId, cues: d.cues });
      if (this.undoStack.length > 100) this.undoStack.shift();
    }
    try {
      const saved = await api.saveCues(path, d.trackId, cues);
      if (this.detail?.trackId === d.trackId) this.detail = { ...this.detail, cues: saved, edited: true };
      const hot: (CueSummary["hot"][number])[] = Array(8).fill(null);
      for (const c of saved) if (c.slot >= 1 && c.slot <= 8) hot[c.slot - 1] = c.color;
      this.cues[d.trackId] = hot;
      this.pending = new Set([...this.pending, d.trackId]);
    } catch (e) {
      this.error = String(e);
    }
  }

  async undo() {
    const last = this.undoStack.pop();
    if (!last || last.trackId !== this.detail?.trackId) {
      if (last) this.undoStack.push(last);
      return;
    }
    await this.setCues(last.cues, false);
  }

  /** Throw away the edits of the selected track. */
  async revertCues() {
    const d = this.detail;
    if (!d || !this.stickPath) return;
    await api.revertCues(this.stickPath, d.trackId);
    this.detail = null;
    delete this.cues[d.trackId];
    this.pending.delete(d.trackId);
    this.pending = new Set(this.pending);
    await this.selectTrack(d.trackId, true);
    this.loadCues([d.trackId]);
  }

  async writeCuesToStick() {
    if (!this.stickPath) return;
    try {
      const r = await api.writeCues(this.stickPath);
      this.notice =
        r.failed.length === 0
          ? `Cue zapsány u ${r.written} skladeb. Záloha původních souborů: ${r.backupDir.replace(/^\/Users\/[^/]+/, "~")}`
          : `Zapsáno ${r.written}, nepovedlo se ${r.failed.length}: ${r.failed.join("; ")}`;
      await this.loadPending();
      if (this.selectedTrack != null) await this.selectTrack(this.selectedTrack, true);
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadSmart() {
    const id = this.selectedTrack;
    if (id == null || !this.stickPath) return;
    try {
      const s = await api.smartCue(this.stickPath, id);
      if (this.selectedTrack === id) {
        this.smart = s;
        this.smartOpen = true;
      }
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Smart Cue onto hot cues of every track in the current list. */
  async smartCuePlaylist(replace: boolean) {
    const path = this.stickPath;
    const ids = this.currentTrackIds;
    if (!path || ids.length === 0) return;
    try {
      const r = await api.smartCueApply(path, ids, replace);
      for (const id of ids) delete this.cues[id];
      await this.loadCues(ids);
      await this.loadPending();
      if (this.selectedTrack != null) await this.selectTrack(this.selectedTrack, true);
      this.notice =
        `Smart Cue nastavil hot cue u ${r.applied} skladeb` +
        (r.skipped ? `, ${r.skipped} už hot cue měly a zůstaly beze změny` : "") +
        (r.failed ? `, ${r.failed} nejde rozebrat (chybí beatgrid nebo waveform)` : "") +
        ". Na flashku se zapíšou tlačítkem Zapsat cue.";
    } catch (e) {
      this.error = String(e);
    }
  }

  setQuantize(on: boolean) {
    this.quantize = on;
    localStorage.setItem("quantize", on ? "on" : "off");
  }

  sortedChildren(parent: number): Playlist[] {
    return (this.data?.playlists ?? []).filter((p) => p.parentId === parent).sort((a, b) => a.sortOrder - b.sortOrder);
  }

  select(s: Selection) {
    this.selection = s;
    this.view = "tracks";
    this.trackFilter = "";
    this.loadCues(this.currentTrackIds);
    this.loadAiStatus(this.currentTrackIds);
  }

  async loadAiStatus(ids: number[]) {
    if (!this.stickPath) return;
    try {
      const st = await api.aiStatus(this.stickPath, ids);
      this.aiInstalled = st.installed;
      this.aiAnalyzed = new Set([...this.aiAnalyzed, ...st.analyzed]);
    } catch {
      /* AI is optional */
    }
  }

  /** Run CUE-DETR on tracks without results (background job). */
  async aiAnalyze(ids: number[]) {
    if (!this.stickPath || this.aiJob != null) return;
    const todo = ids.filter((id) => !this.aiAnalyzed.has(id));
    if (todo.length === 0) return;
    try {
      this.aiJob = await api.aiAnalyze(this.stickPath, todo);
    } catch (e) {
      this.error = String(e);
    }
  }

  private async aiFinished(job: JobEvent) {
    this.aiJob = null;
    await this.loadAiStatus(this.currentTrackIds.concat(this.selectedTrack != null ? [this.selectedTrack] : []));
    if (job.state === "error") this.error = job.message;
    else if (job.state === "done") this.notice = `${job.message}. Smart Cue teď používá cue body z AI.`;
    if (this.smartOpen) await this.loadSmart();
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

  async selectTrack(id: number, reload = false) {
    if (id !== this.selectedTrack) {
      this.smart = null;
      this.smartOpen = false;
    }
    this.selectedTrack = id;
    const path = this.stickPath;
    if (!path) return;
    if (this.detail?.trackId === id && !reload) return;
    try {
      const d = await api.trackDetail(path, id);
      if (this.selectedTrack === id) this.detail = d;
    } catch (e) {
      this.error = String(e);
    }
  }
}

export const app = new AppState();
