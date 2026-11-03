import { listen } from "@tauri-apps/api/event";
import { gridBeats, normalize, type Grid } from "./grid";
import {
  api,
  libApi,
  type Cue,
  type CueSummary,
  type JobEvent,
  type LibraryData,
  type PairRequest,
  type PaletteColor,
  type Playlist,
  type SmartCue,
  type StickData,
  type StickInfo,
  type Track,
  type TrackDetail,
  type VerifyResult,
} from "./api";

export type Selection =
  | { kind: "all" }
  | { kind: "playlist"; id: number }
  | { kind: "history"; id: number }
  | { kind: "player"; player: string };

export type View = "tracks" | "health" | "planner";

/** Where the app is working: the library on this Mac, or a stick. */
export type Source = "library" | "stick";

/** The track loaded on the deck and where it comes from. It stays loaded
 *  (and keeps playing) when the app switches between library and stick. */
export interface DeckTrack {
  track: Track;
  source: Source;
  stickPath: string | null;
}

class AppState {
  source = $state<Source>("library");
  sticks = $state<StickInfo[]>([]);
  stickPath = $state<string | null>(null);
  data = $state<StickData | null>(null);
  library = $state<LibraryData | null>(null);
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
  /** Memory cue count per track, for the table. */
  memCount = $state<Record<number, number>>({});
  selectedTrack = $state<number | null>(null);
  deck = $state<DeckTrack | null>(null);
  detailOpen = $state(false);
  detail = $state<TrackDetail | null>(null);

  jobs = $state<JobEvent[]>([]);
  exportOpen = $state(false);
  syncOpen = $state(false);
  /** Player settings sheet: `undefined` closed, `null` the DJ's profile, else a stick path. */
  settingsFor = $state<string | null | undefined>(undefined);
  formatOpen = $state(false);
  /** Sync with Stickwise on other Macs. */
  peerOpen = $state(false);
  /** Another Mac asks to pair, waiting for the DJ's answer. */
  pairRequest = $state<PairRequest | null>(null);
  /** Full-screen tool launcher. */
  toolsOpen = $state(false);
  importing = $state(false);
  /** Folders dropped from Finder being read into the library. */
  addingFolders = $state(false);
  /** Playlist being renamed in the tree. */
  renaming = $state<number | null>(null);

  /** Stick tracks with cue edits not yet written to the stick. */
  pending = $state<Set<number>>(new Set());
  palette = $state<PaletteColor[]>([]);
  quantize = $state(localStorage.getItem("quantize") !== "off");
  writeOpen = $state(false);
  /** Smart Cue suggestions for the selected track, shown on the waveform. */
  smart = $state<SmartCue[] | null>(null);
  smartOpen = $state(false);
  /** Beat grid edit mode on the deck, and its click track. */
  gridOpen = $state(false);
  metronome = $state(false);
  private gridUndo: { trackId: number; grid: Grid }[] = [];
  private gridSave: ReturnType<typeof setTimeout> | undefined;
  /** CUE-DETR sidecar installed, and which tracks it has analysed. */
  aiInstalled = $state(false);
  aiAnalyzed = $state<Set<number>>(new Set());
  aiJob = $state<number | null>(null);
  /** Own analysis running, and tracks waiting for the next round. */
  analyzeJob = $state<number | null>(null);
  private analyzeQueue: { ids: number[]; again: boolean }[] = [];
  notice = $state<string | null>(null);
  private undoStack: { trackId: number; cues: Cue[] }[] = [];

  isLibrary = $derived(this.source === "library");
  /** The deck's track comes from what is open now. */
  deckHere = $derived(
    !!this.deck && this.deck.source === this.source && (this.isLibrary || this.deck.stickPath === this.stickPath),
  );
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

  currentPlaylist = $derived.by((): Playlist | undefined => {
    const s = this.selection;
    return s?.kind === "playlist" ? this.data?.playlists.find((p) => p.id === s.id) : undefined;
  });

  currentTrackIds = $derived.by((): number[] => {
    const s = this.selection;
    const d = this.data;
    if (!s || !d) return [];
    if (s.kind === "all") return d.tracks.map((t) => t.id);
    if (s.kind === "playlist") return d.playlists.find((p) => p.id === s.id)?.trackIds ?? [];
    if (s.kind === "history") return d.history.find((p) => p.id === s.id)?.trackIds ?? [];
    return this.verify?.players.find((p) => p.player === s.player)?.unplayable ?? [];
  });

  currentTracks = $derived.by((): Track[] => {
    const q = this.trackFilter.trim().toLowerCase();
    return this.currentTrackIds
      .map((id) => this.trackById.get(id))
      .filter((t): t is Track => !!t)
      .filter((t) => !q || `${t.title} ${t.artist} ${t.album} ${t.genre}`.toLowerCase().includes(q));
  });

  currentTitle = $derived.by(() => {
    const s = this.selection;
    if (!s || !this.data) return "";
    if (s.kind === "all") return "Celá knihovna";
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
      if (e.payload.id === this.analyzeJob && e.payload.state !== "running") this.analyzeFinished(e.payload);
    });
    await listen<PairRequest>("peer-pair-request", (e) => (this.pairRequest = e.payload));
    await listen<number>("peer-pair-gone", (e) => {
      if (this.pairRequest?.request === e.payload) this.pairRequest = null;
    });
    this.palette = await api.cuePalette();
    await this.openLibrary();
    await this.refreshSticks();
    setInterval(() => this.refreshSticks(), 2500);
  }

  async refreshSticks() {
    try {
      const list = await api.listSticks();
      this.sticks = list;
      const deckPath = this.deck?.stickPath;
      if (deckPath && !list.some((s) => s.path === deckPath)) this.unloadDeck();
      if (this.source === "stick" && this.stickPath && !list.some((s) => s.path === this.stickPath)) {
        await this.openLibrary();
      }
    } catch (e) {
      this.error = String(e);
    }
  }

  private reset() {
    this.data = null;
    this.verify = null;
    this.selection = null;
    this.cues = {};
    // The deck (its detail, Smart Cue and undo) stays loaded.
    this.detailOpen = false;
    this.selectedTrack = null;
    this.aiAnalyzed = new Set();
    this.view = "tracks";
  }

  /** After a source opened: the deck's track is selected again if it is from here. */
  private reselectDeck() {
    if (this.deckHere && this.deck) this.selectedTrack = this.deck.track.id;
  }

  private unloadDeck() {
    this.deck = null;
    this.detail = null;
    this.smart = null;
    this.smartOpen = false;
    this.undoStack = [];
  }

  /** The library as the same shape the stick views use. */
  private libraryView(d: LibraryData): StickData {
    return {
      info: { path: "library", name: "Knihovna", totalBytes: 0, freeBytes: 0, filesystem: "" },
      playlists: d.playlists,
      history: [],
      tracks: d.tracks,
      problems: [],
    };
  }

  async openLibrary() {
    this.source = "library";
    this.stickPath = null;
    this.reset();
    this.loading = true;
    try {
      await this.reloadLibrary();
      const firstSet = this.data?.playlists.find((p) => p.isSet);
      this.select(firstSet ? { kind: "playlist", id: firstSet.id } : { kind: "all" });
      this.reselectDeck();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  /** Re-read the library, keeping the selection. */
  async reloadLibrary() {
    const d = await libApi.data();
    this.library = d;
    if (this.source === "library") {
      this.data = this.libraryView(d);
      if (this.selection?.kind === "playlist" && !d.playlists.some((p) => this.selection?.kind === "playlist" && p.id === this.selection.id)) {
        this.selection = { kind: "all" };
      }
    }
  }

  async open(path: string) {
    if (this.source === "stick" && this.stickPath === path && this.data) return;
    this.source = "stick";
    this.reset();
    this.stickPath = path;
    this.loading = true;
    this.error = null;
    try {
      this.data = await api.openStick(path);
      const first = this.sortedChildren(0).find((p) => !p.isFolder) ?? this.data.playlists.find((p) => !p.isFolder);
      if (first) this.select({ kind: "playlist", id: first.id });
      this.reselectDeck();
      this.loadPending();
      this.runVerify();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  // ---- library editing ------------------------------------------------

  async importRekordbox(source: string) {
    this.importing = true;
    try {
      const r = await libApi.import(source);
      await this.reloadLibrary();
      this.cues = {};
      this.loadCues(this.currentTrackIds);
      this.notice =
        `Z rekordboxu: ${r.tracksAdded} nových a ${r.tracksUpdated} aktualizovaných skladeb, ${r.playlists} playlistů, ${r.cues} cue.` +
        (r.cuesKept ? ` Tvoje úpravy cue u ${r.cuesKept} skladeb zůstaly.` : "") +
        (r.playlistsKept ? ` ${r.playlistsKept} playlistů upravených tady zůstalo beze změny.` : "");
    } catch (e) {
      this.error = String(e);
    } finally {
      this.importing = false;
    }
  }

  /**
   * Folders (and audio files) dropped from Finder: each folder becomes a
   * playlist of its name. Dropped on a folder they go inside it; loose
   * files dropped on a playlist are added to it.
   */
  async addFolders(paths: string[], target: number | null) {
    if (paths.length === 0 || this.addingFolders) return;
    this.addingFolders = true;
    try {
      if (!this.isLibrary) await this.openLibrary();
      const t = target != null ? this.library?.playlists.find((p) => p.id === target) : undefined;
      const parent = t ? (t.isFolder ? t.id : t.parentId) : 0;
      const playlist = t && !t.isFolder ? t.id : null;
      const r = await libApi.addFolders(paths, parent, playlist);
      await this.reloadLibrary();
      const show = r.firstPlaylist ?? (r.loose.length && playlist != null ? playlist : null);
      if (show != null) this.select({ kind: "playlist", id: show });
      else if (r.loose.length) this.select({ kind: "all" });
      const lists = r.playlistsCreated + r.playlistsUpdated;
      const parts = [
        lists ? `${r.playlistsCreated} nových playlistů` + (r.playlistsUpdated ? `, ${r.playlistsUpdated} doplněných` : "") : "",
        `${r.tracksAdded} nových skladeb` + (r.tracksKnown ? ` (${r.tracksKnown} už v knihovně bylo)` : ""),
      ].filter(Boolean);
      if (lists === 0 && r.tracksAdded + r.tracksKnown === 0) {
        this.error = `Ve složce není žádná hudba (MP3, WAV, AIFF, FLAC, M4A): ${r.ignored.join(", ")}`;
        return;
      }
      this.notice =
        `Ze složek: ${parts.join(", ")}.` +
        (r.ignored.length ? ` Bez hudby, vynecháno: ${r.ignored.join(", ")}.` : "") +
        (r.added.length ? " Analyzuju je na pozadí: waveform, beatgrid, tónina." : "");
      // New tracks are analysed right away.
      this.analyzeTracks(r.added);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.addingFolders = false;
    }
  }

  async createPlaylist(parent: number, kind: "playlist" | "folder" | "set") {
    const name = kind === "set" ? "Nový set" : kind === "folder" ? "Nová složka" : "Nový playlist";
    try {
      const id = await libApi.createPlaylist(parent, name, kind);
      await this.reloadLibrary();
      if (kind !== "folder") this.select({ kind: "playlist", id });
      this.renaming = id;
    } catch (e) {
      this.error = String(e);
    }
  }

  async renamePlaylist(id: number, name: string) {
    this.renaming = null;
    if (!name.trim()) return;
    await libApi.renamePlaylist(id, name.trim()).catch((e) => (this.error = String(e)));
    await this.reloadLibrary();
  }

  async deletePlaylist(id: number) {
    await libApi.deletePlaylist(id).catch((e) => (this.error = String(e)));
    if (this.selection?.kind === "playlist" && this.selection.id === id) this.selection = { kind: "all" };
    await this.reloadLibrary();
  }

  async movePlaylist(id: number, parent: number, index: number) {
    await libApi.movePlaylist(id, parent, index).catch((e) => (this.error = String(e)));
    await this.reloadLibrary();
  }

  /** Siblings `node` moves among: sets with sets, the rest with the rest. */
  private moveGroup(node: Playlist): Playlist[] {
    return this.sortedChildren(node.parentId).filter((p) => p.isSet === node.isSet);
  }

  canShiftPlaylist(id: number, dir: -1 | 1): boolean {
    const node = this.data?.playlists.find((p) => p.id === id);
    if (!node || !this.isLibrary) return false;
    const group = this.moveGroup(node);
    const i = group.findIndex((p) => p.id === id);
    return i + dir >= 0 && i + dir < group.length;
  }

  /** Move a playlist one place up (-1) or down (1) among its siblings. */
  async shiftPlaylist(id: number, dir: -1 | 1) {
    const node = this.data?.playlists.find((p) => p.id === id);
    if (!node || !this.canShiftPlaylist(id, dir)) return;
    const group = this.moveGroup(node);
    const neighbour = group[group.findIndex((p) => p.id === id) + dir];
    await this.placePlaylist(id, neighbour.id, dir > 0);
  }

  /** Move a playlist right before (or after) `target`, into its folder. */
  async placePlaylist(id: number, target: number, after: boolean) {
    const ps = this.data?.playlists ?? [];
    const t = ps.find((p) => p.id === target);
    if (!t || id === target) return;
    // Not into itself or its own subfolders.
    for (let p: Playlist | undefined = t; p; p = ps.find((x) => x.id === p!.parentId)) {
      if (p.id === id) return;
    }
    const rest = this.sortedChildren(t.parentId).filter((p) => p.id !== id);
    const index = rest.findIndex((p) => p.id === target) + (after ? 1 : 0);
    // Shown at once; the library confirms on reload.
    const order = [...rest.slice(0, index).map((p) => p.id), id, ...rest.slice(index).map((p) => p.id)];
    for (const p of ps) {
      const i = order.indexOf(p.id);
      if (p.id === id) p.parentId = t.parentId;
      if (i >= 0) p.sortOrder = i;
    }
    await this.movePlaylist(id, t.parentId, index);
  }

  async setPlaylistTracks(id: number, ids: number[]) {
    await libApi.setTracks(id, ids).catch((e) => (this.error = String(e)));
    await this.reloadLibrary();
  }

  /** Append tracks to a playlist, skipping ones already in it. */
  async addToPlaylist(id: number, ids: number[]) {
    const p = this.data?.playlists.find((x) => x.id === id);
    if (!p || p.isFolder) return;
    const add = ids.filter((t) => !p.trackIds.includes(t));
    if (add.length === 0) return;
    await this.setPlaylistTracks(id, [...p.trackIds, ...add]);
    this.notice = `Přidáno do „${p.name}“: ${add.length}`;
  }

  /** A new set holding `ids`, ready to be named in the tree. */
  async newSetWith(ids: number[]) {
    try {
      const id = await libApi.createPlaylist(0, "Nový set", "set");
      await libApi.setTracks(id, ids);
      await this.reloadLibrary();
      this.renaming = id;
      this.notice = `Nový set s ${ids.length} skladbami. Pojmenuj ho vlevo v seznamu setů.`;
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Remove the rows at `positions` from the selected playlist. */
  async removeFromPlaylist(positions: number[]) {
    const p = this.currentPlaylist;
    if (!p) return;
    const drop = new Set(positions);
    await this.setPlaylistTracks(p.id, p.trackIds.filter((_, i) => !drop.has(i)));
  }

  /** Move the row at `from` to `to` in the selected playlist. */
  async reorder(from: number, to: number) {
    const p = this.currentPlaylist;
    if (!p || from === to) return;
    const ids = [...p.trackIds];
    const [t] = ids.splice(from, 1);
    ids.splice(to > from ? to - 1 : to, 0, t);
    await this.setPlaylistTracks(p.id, ids);
  }

  // ---- cues -----------------------------------------------------------

  async loadPending() {
    if (!this.stickPath) return;
    this.pending = new Set(await api.pendingCues(this.stickPath));
  }

  /** Replace the cues of the selected track: saved in the library, or kept until written to the stick. */
  async setCues(cues: Cue[], undoable = true) {
    const d = this.detail;
    const deck = this.deck;
    if (!d || !deck) return;
    const lib = deck.source === "library";
    const here = this.deckHere;
    if (undoable) {
      this.undoStack.push({ trackId: d.trackId, cues: d.cues });
      if (this.undoStack.length > 100) this.undoStack.shift();
    }
    try {
      const saved = lib
        ? await libApi.saveCues(d.trackId, cues)
        : await api.saveCues(deck.stickPath!, d.trackId, cues);
      if (this.detail === d || this.detail?.trackId === d.trackId) this.detail = { ...this.detail!, cues: saved, edited: !lib };
      // The table and pending list show the source that is open.
      if (!here) return;
      const hot: CueSummary["hot"][number][] = Array(8).fill(null);
      for (const c of saved) if (c.slot >= 1 && c.slot <= 8) hot[c.slot - 1] = c.color;
      this.cues[d.trackId] = hot;
      this.memCount[d.trackId] = saved.filter((c) => c.slot === 0).length;
      if (!lib) this.pending = new Set([...this.pending, d.trackId]);
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

  /** Throw away the stick edits of the selected track. */
  async revertCues() {
    const d = this.detail;
    const path = this.deck?.stickPath;
    if (!d || !path) return;
    await api.revertCues(path, d.trackId);
    await this.reloadDeck();
    if (!this.deckHere) return;
    delete this.cues[d.trackId];
    this.pending.delete(d.trackId);
    this.pending = new Set(this.pending);
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
      await this.reloadDeck();
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadSmart() {
    const deck = this.deck;
    if (!deck) return;
    const id = deck.track.id;
    try {
      const s = deck.source === "library" ? await libApi.smartCue(id) : await api.smartCue(deck.stickPath!, id);
      if (this.deck === deck) {
        this.smart = s;
        this.smartOpen = true;
      }
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Smart Cue onto hot cues of every track in the current list. */
  async smartCuePlaylist(replace: boolean) {
    const ids = this.currentTrackIds;
    if (ids.length === 0) return;
    try {
      const r = this.isLibrary
        ? await libApi.smartCueApply(ids, replace)
        : await api.smartCueApply(this.stickPath!, ids, replace);
      for (const id of ids) delete this.cues[id];
      await this.loadCues(ids);
      await this.loadPending();
      if (this.deckHere) await this.reloadDeck();
      this.notice =
        `Smart Cue nastavil hot cue u ${r.applied} skladeb` +
        (r.skipped ? `, ${r.skipped} už hot cue měly a zůstaly beze změny` : "") +
        (r.failed ? `, ${r.failed} nejde rozebrat (chybí beatgrid nebo waveform)` : "") +
        (this.isLibrary ? ". Na flashku se dostanou synchronizací." : ". Na flashku se zapíšou tlačítkem Zapsat cue.");
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
    const ids = this.currentTrackIds;
    // The whole library can be large: cue squares load as rows are needed.
    this.loadCues(s.kind === "all" ? ids.slice(0, 300) : ids);
    this.loadAiStatus(s.kind === "all" ? ids.slice(0, 300) : ids);
  }

  async loadAiStatus(ids: number[]) {
    if (!this.isLibrary && !this.stickPath) return;
    try {
      const st = this.isLibrary ? await libApi.aiStatus(ids) : await api.aiStatus(this.stickPath!, ids);
      this.aiInstalled = st.installed;
      this.aiAnalyzed = new Set([...this.aiAnalyzed, ...st.analyzed]);
    } catch {
      /* AI is optional */
    }
  }

  /** Run CUE-DETR on tracks without results (background job). */
  async aiAnalyze(ids: number[]) {
    if (this.aiJob != null) return;
    const todo = ids.filter((id) => !this.aiAnalyzed.has(id));
    if (todo.length === 0) return;
    try {
      this.aiJob = this.isLibrary ? await libApi.aiAnalyze(todo) : await api.aiAnalyze(this.stickPath!, todo);
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Library tracks without analysis (and with their file), of `ids`. */
  unanalyzed(ids: number[], again = false): number[] {
    const byId = new Map((this.library?.tracks ?? []).map((t) => [t.id, t]));
    return [...new Set(ids)].filter((id) => {
      const t = byId.get(id);
      return t && !t.missing && (t.analyzed === false || (again && t.ownAnalysis));
    });
  }

  /** Tracks of a playlist, or of everything inside a folder. */
  playlistTracks(id: number): number[] {
    const ps = this.library?.playlists ?? [];
    const p = ps.find((x) => x.id === id);
    if (!p) return [];
    return p.isFolder ? ps.filter((c) => c.parentId === id).flatMap((c) => this.playlistTracks(c.id)) : p.trackIds;
  }

  /** Analyse library tracks in the background: waveforms, beat grid, key. */
  async analyzeTracks(ids: number[], again = false) {
    const todo = this.unanalyzed(ids, again);
    if (todo.length === 0) return;
    if (this.analyzeJob != null) {
      this.analyzeQueue.push({ ids: todo, again });
      this.notice = `Ve frontě na analýzu: ${todo.length} skladeb`;
      return;
    }
    try {
      this.analyzeJob = await libApi.analyze(todo, again);
    } catch (e) {
      this.error = String(e);
    }
  }

  private async analyzeFinished(job: JobEvent) {
    this.analyzeJob = null;
    await this.reloadLibrary();
    if (this.deck?.source === "library") {
      this.deck.track = this.library?.tracks.find((t) => t.id === this.deck!.track.id) ?? this.deck.track;
      await this.reloadDeck();
    }
    if (job.state === "error") this.error = job.message;
    else if (job.state === "done") this.notice = `${job.message}. Na flashku půjdou s waveformem i gridem.`;
    for (const q of this.analyzeQueue.splice(0)) await this.analyzeTracks(q.ids, q.again);
  }

  private async aiFinished(job: JobEvent) {
    this.aiJob = null;
    await this.loadAiStatus(this.currentTrackIds.concat(this.selectedTrack != null ? [this.selectedTrack] : []));
    if (job.state === "error") this.error = job.message;
    else if (job.state === "done") this.notice = `${job.message}. Smart Cue teď používá cue body z AI.`;
    if (this.smartOpen) await this.loadSmart();
  }

  async loadCues(ids: number[]) {
    const missing = ids.filter((id) => !(id in this.cues));
    if (missing.length === 0 || (!this.isLibrary && !this.stickPath)) return;
    try {
      const list = this.isLibrary ? await libApi.cueSummary(missing) : await api.cueSummary(this.stickPath!, missing);
      for (const c of list) {
        this.cues[c.trackId] = c.hot;
        this.memCount[c.trackId] = c.memory;
      }
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

  /** Select a row and load its track onto the deck. */
  async selectTrack(id: number, reload = false) {
    const track = this.trackById.get(id);
    if (!track || (!this.isLibrary && !this.stickPath)) return;
    this.selectedTrack = id;
    if (!this.deckHere || this.deck!.track.id !== id) {
      this.flushGrid();
      this.deck = { track, source: this.source, stickPath: this.stickPath };
      this.detail = null;
      this.smart = null;
      this.smartOpen = false;
    } else if (!reload && this.detail?.trackId === id) {
      return;
    }
    await this.reloadDeck();
  }

  /** Read the deck track's waveform and cues again from its source. */
  /** The deck's grid can be edited: a library track with a grid. */
  get gridEditable(): boolean {
    return this.deck?.source === "library" && !!this.detail?.grid;
  }

  /** Change the deck's beat grid: shown at once, saved a moment later. */
  setGrid(next: Grid, undoable = true) {
    const d = this.detail;
    const deck = this.deck;
    if (!d?.grid || !deck || deck.source !== "library" || !(next.bpm >= 20 && next.bpm <= 400)) return;
    if (undoable) {
      this.gridUndo.push({ trackId: d.trackId, grid: { bpm: d.grid.bpm, anchorMs: d.grid.anchorMs } });
      if (this.gridUndo.length > 200) this.gridUndo.shift();
    }
    const g = normalize(next);
    const durationMs = (deck.track.duration + 2) * 1000;
    this.detail = { ...d, beats: gridBeats(g, durationMs), grid: { ...g, edited: true } };
    // The tempo shown everywhere follows the grid.
    const bpm = Math.round(g.bpm * 100) / 100;
    deck.track.bpm = bpm;
    const row = this.library?.tracks.find((t) => t.id === d.trackId);
    if (row) row.bpm = bpm;
    const id = d.trackId;
    clearTimeout(this.gridSave);
    this.gridSave = setTimeout(async () => {
      this.gridSave = undefined;
      try {
        await libApi.setGrid(id, g.bpm, g.anchorMs);
        if (this.smartOpen && this.deck?.track.id === id) await this.loadSmart();
      } catch (e) {
        this.error = String(e);
      }
    }, 300);
  }

  /** Save a pending grid change now. */
  async flushGrid() {
    if (this.gridSave === undefined) return;
    clearTimeout(this.gridSave);
    this.gridSave = undefined;
    const d = this.detail;
    if (d?.grid?.edited) await libApi.setGrid(d.trackId, d.grid.bpm, d.grid.anchorMs).catch((e) => (this.error = String(e)));
  }

  undoGrid() {
    const last = this.gridUndo.pop();
    if (!last || last.trackId !== this.detail?.trackId) {
      if (last) this.gridUndo.push(last);
      return;
    }
    this.setGrid(last.grid, false);
  }

  get canUndoGrid(): boolean {
    const last = this.gridUndo[this.gridUndo.length - 1];
    return !!last && last.trackId === this.detail?.trackId;
  }

  /** Back to the analysed grid. */
  async resetGrid() {
    const d = this.detail;
    if (!d || this.deck?.source !== "library") return;
    clearTimeout(this.gridSave);
    this.gridSave = undefined;
    if (d.grid) this.gridUndo.push({ trackId: d.trackId, grid: { bpm: d.grid.bpm, anchorMs: d.grid.anchorMs } });
    try {
      await libApi.resetGrid(d.trackId);
      await this.reloadLibrary();
      const t = this.library?.tracks.find((x) => x.id === d.trackId);
      if (t && this.deck) this.deck.track.bpm = t.bpm;
      await this.reloadDeck();
      if (this.smartOpen) await this.loadSmart();
    } catch (e) {
      this.error = String(e);
    }
  }

  async reloadDeck() {
    const deck = this.deck;
    if (!deck) return;
    const id = deck.track.id;
    try {
      const d = deck.source === "library" ? await libApi.trackDetail(id) : await api.trackDetail(deck.stickPath!, id);
      if (this.deck === deck) this.detail = d;
    } catch (e) {
      this.error = String(e);
    }
  }
}

export const app = new AppState();
