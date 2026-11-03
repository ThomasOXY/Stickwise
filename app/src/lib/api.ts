import { invoke } from "@tauri-apps/api/core";

export interface StickInfo {
  path: string;
  name: string;
  totalBytes: number;
  freeBytes: number;
  filesystem: string;
}

export interface Playlist {
  id: number;
  parentId: number;
  name: string;
  isFolder: boolean;
  sortOrder: number;
  trackIds: number[];
  /** Library only: a DJ set. */
  isSet?: boolean;
  /** Library only: `local` or `rekordbox`. */
  source?: string;
}

export interface Track {
  id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  bpm: number;
  key: string;
  duration: number;
  format: string;
  filePath: string;
  absPath: string;
  rating: number;
  fileSize: number;
  artwork: string | null;
  /** `YYYY-MM-DD`, empty if unknown. */
  dateAdded: string;
  /** Library only: analysed by this app, not by rekordbox. */
  ownAnalysis?: boolean;
  /** Library only: audio file not found. */
  missing?: boolean;
  /** Library only: has rekordbox analysis. */
  analyzed?: boolean;
}

export interface LibraryData {
  tracks: Track[];
  playlists: Playlist[];
  rekordboxSources: string[];
  lastImport: string | null;
}

export interface ImportReport {
  tracksAdded: number;
  tracksUpdated: number;
  cues: number;
  cuesKept: number;
  playlists: number;
  playlistsKept: number;
}

export interface FolderReport {
  playlistsCreated: number;
  playlistsUpdated: number;
  tracksAdded: number;
  tracksKnown: number;
  /** Ids of the tracks added now. */
  added: number[];
  firstPlaylist: number | null;
  loose: number[];
  ignored: string[];
}

export interface SyncPlan {
  tracks: number;
  playlists: number;
  copyFiles: number;
  copyBytes: number;
  reuseFiles: number;
  freeBytes: number;
  missingAudio: string[];
  noAnalysis: string[];
}

export interface StickData {
  info: StickInfo;
  playlists: Playlist[];
  history: Playlist[];
  tracks: Track[];
  problems: string[];
}

export type Rgb = [number, number, number];

export interface CueSummary {
  trackId: number;
  hot: (Rgb | null)[];
  /** Memory cues and memory loops. */
  memory: number;
}

export interface Waveform {
  kind: "three-band" | "color" | "mono";
  entryLen: number;
  data: number[];
}

export interface Cue {
  /** 0 = memory cue / loop, 1..8 = hot cue A..H. */
  slot: number;
  timeMs: number;
  loopEndMs: number | null;
  loopBeats: [number, number] | null;
  comment: string;
  hotColor: number;
  hotLed: Rgb | null;
  memoryColor: number;
  activeLoop: boolean;
  /** Display colour from the backend. */
  color: Rgb | null;
}

export interface PaletteColor {
  code: number;
  color: Rgb;
}

export interface SmartCue {
  kind: "mix-in" | "drop" | "break" | "build" | "up" | "down" | "outro" | "mix-out" | "cue";
  timeMs: number;
  bar: number;
  label: string;
  confidence: number;
  colorCode: number;
  color: Rgb;
  inPlan: boolean;
  /** Placed by CUE-DETR. */
  ai: boolean;
}

export interface AiStatus {
  installed: boolean;
  analyzed: number[];
}

export interface SmartCueBatch {
  applied: number;
  skipped: number;
  failed: number;
}

export interface CueWriteResult {
  written: number;
  failed: string[];
  backupDir: string;
}

export interface AudioInfo {
  container: string;
  codec: string;
  bits: number | null;
  sampleRate: number | null;
  channels: number | null;
  description: string;
}

export interface TrackDetail {
  trackId: number;
  previews: Waveform[];
  details: Waveform[];
  cues: Cue[];
  edited: boolean;
  beats: [number, number][];
  /** The grid as tempo and one bar one; `edited` when the DJ changed it. */
  grid: { bpm: number; anchorMs: number; edited: boolean } | null;
  audio: AudioInfo | null;
  fileSize: number;
}

export interface PlayerResult {
  player: string;
  filesystemOk: boolean;
  filesystemNote: string;
  unplayable: number[];
  formatNote: string;
}

export interface Issue {
  trackId: number;
  title: string;
  filePath: string;
  playlists: string[];
  severity: "error" | "warning";
  kind: string;
  message: string;
  audio: string | null;
}

export interface VerifyResult {
  tracks: number;
  filesystem: string;
  players: PlayerResult[];
  issues: Issue[];
  appleDouble: number;
  dbProblems: string[];
  formats: [string, number][];
  seconds: number;
}

export interface ExtractRequest {
  stick: string;
  dest: string;
  playlists: number[];
  numbered: boolean;
  artistTitle: boolean;
  m3u: boolean;
  history: boolean;
  duplicates: "clone" | "copy" | "symlink";
}

export interface JobEvent {
  id: number;
  label: string;
  state: "running" | "done" | "cancelled" | "error";
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
  current: string;
  message: string;
  dest: string;
  /** Step of a multi-step job (`copy`, `analysis`, `erase`, …). */
  phase: string;
  /** The whole job, 0 to 1, when the job knows it. */
  fraction: number | null;
}

export interface EngineStatus {
  /** Schema of the Denon library on the stick, e.g. `3.0.2`. */
  version: string | null;
  minFirmware: string | null;
  tracks: number;
}

export interface SettingField {
  key: string;
  /** `player`, `display`, `light`, `waveform`, `mixer`. */
  group: string;
  label: string;
  options: { value: number; label: string }[];
  default: number;
}

export type SettingValues = Record<string, number>;

export interface PlayerSettings {
  values: SettingValues;
  /** `stick`, `profile` (saved in the app), `rekordbox`, `default`. */
  source: "stick" | "profile" | "rekordbox" | "default";
  profile: SettingValues;
  playerSaved: string[];
}

export interface Disk {
  id: string;
  media: string;
  size: number;
  bus: string;
  removable: boolean;
  volumes: { name: string; mountPoint: string | null; content: string }[];
  blocked: string | null;
}

export const playerApi = {
  schema: () => invoke<SettingField[]>("settings_schema"),
  get: (stick: string | null) => invoke<PlayerSettings>("settings_get", { stick }),
  save: (values: SettingValues) => invoke<void>("settings_save", { values }),
  write: (stick: string, values: SettingValues) => invoke<string>("settings_write", { stick, values }),
  listDisks: () => invoke<Disk[]>("list_disks"),
  formatStart: (disk: string, filesystem: "fat32" | "exfat", name: string, confirm: string, writeSettings: boolean) =>
    invoke<number>("format_start", { disk, filesystem, name, confirm, writeSettings }),
};

export interface PeerSettings {
  /** Where music from another Mac is saved. */
  musicDir: string;
  defaultMusicDir: boolean;
  fingerprinted: number;
  total: number;
  /** Tracks without a fingerprint whose file can't be read: skipped. */
  skipped: number;
  /** Tracks without a fingerprint whose file is not there (a drive not plugged in). */
  waiting: number;
  /** Mirror with paired Macs by itself when they come on the network. */
  autoMirror: boolean;
}

export interface Peer {
  id: string;
  name: string;
  /** Seen on the network now. */
  online: boolean;
  paired: boolean;
}

export interface PeerList {
  /** This Mac's name. */
  name: string;
  peers: Peer[];
  /** Why sync between Macs is not running. */
  error: string | null;
}

export interface PairRequest {
  request: number;
  peer: Peer;
  code: string;
}

export interface PairResult {
  peer: Peer;
  accepted: boolean;
  message: string;
}

/** What another Mac offers to send. */
export interface OfferRequest {
  request: number;
  from: string;
  summary: {
    /** Playlists and sets (not folders). */
    playlists: string[];
    sets: number;
    tracks: number;
    /** Tracks this Mac does not have yet, and their size. */
    newTracks: number;
    newBytes: number;
  };
}

export interface ApplyReport {
  tracksAdded: number;
  tracksUpdated: number;
  cuesUpdated: number;
  playlistsAdded: number;
  playlistsUpdated: number;
  playlistsDeleted: number;
  skipped: string[];
}

/** What mirroring would do to one Mac's library. */
export interface MirrorChanges {
  report: ApplyReport;
  newBytes: number;
  /** Playlists that cannot go along: some of their tracks are missing. */
  leftOut: string[];
}

/** A playlist deleted on one Mac that the other still has. */
export interface Deletion {
  uid: string;
  name: string;
  isFolder: boolean;
  isSet: boolean;
  tracks: number;
  /** Unix ms. */
  deleted: number;
}

export interface MirrorRequest {
  request: number;
  peer: string;
  preview: {
    here: MirrorChanges;
    there: MirrorChanges;
    /** Playlists here that the other Mac deleted. */
    deleteHere: Deletion[];
    /** Playlists the other Mac has that were deleted here. */
    deleteThere: Deletion[];
    roomHere: boolean;
  };
}

export const peerApi = {
  settings: () => invoke<PeerSettings>("peer_settings"),
  setMusicDir: (path: string | null) => invoke<PeerSettings>("peer_set_music_dir", { path }),
  setAutoMirror: (on: boolean) => invoke<PeerSettings>("peer_set_auto_mirror", { on }),
  list: () => invoke<PeerList>("peer_list"),
  /** Returns the code both Macs show. */
  pair: (id: string) => invoke<string>("peer_pair", { id }),
  answer: (request: number, accept: boolean) => invoke<void>("peer_answer", { request, accept }),
  unpair: (id: string) => invoke<void>("peer_unpair", { id }),
  /** Milliseconds there and back. */
  ping: (id: string) => invoke<number>("peer_ping", { id }),
  /** Send playlists (folders whole) and tracks; returns the job id. */
  send: (id: string, playlistIds: number[], trackIds: number[]) => invoke<number>("peer_send", { id, playlistIds, trackIds }),
  answerOffer: (request: number, accept: boolean) => invoke<void>("peer_answer_offer", { request, accept }),
  /** Mirror the library both ways; returns the job id. The preview comes as an event. */
  mirror: (id: string) => invoke<number>("peer_mirror", { id }),
  /** `null` calls the mirror off. */
  mirrorAnswer: (request: number, choice: { deleteHere: string[]; deleteThere: string[] } | null) =>
    invoke<void>("peer_mirror_answer", { request, choice }),
};

export const libApi = {
  data: () => invoke<LibraryData>("library_data"),
  import: (source: string) => invoke<ImportReport>("library_import", { source }),
  createPlaylist: (parent: number, name: string, kind: "playlist" | "folder" | "set") =>
    invoke<number>("library_create_playlist", { parent, name, kind }),
  renamePlaylist: (id: number, name: string) => invoke<void>("library_rename_playlist", { id, name }),
  deletePlaylist: (id: number) => invoke<void>("library_delete_playlist", { id }),
  movePlaylist: (id: number, parent: number, index: number) =>
    invoke<void>("library_move_playlist", { id, parent, index }),
  setTracks: (id: number, trackIds: number[]) => invoke<void>("library_set_tracks", { id, trackIds }),
  /** Each group: sibling playlists in their new order. */
  orderPlaylists: (groups: number[][]) => invoke<void>("library_order_playlists", { groups }),
  setGrid: (id: number, bpm: number, anchorMs: number) => invoke<void>("library_set_grid", { id, bpm, anchorMs }),
  resetGrid: (id: number) => invoke<void>("library_reset_grid", { id }),
  addFolders: (paths: string[], parent: number, playlist: number | null) =>
    invoke<FolderReport>("library_add_folders", { paths, parent, playlist }),
  trackDetail: (id: number) => invoke<TrackDetail>("library_track_detail", { id }),
  saveCues: (id: number, cues: Cue[]) => invoke<Cue[]>("library_save_cues", { id, cues }),
  cueSummary: (trackIds: number[]) => invoke<CueSummary[]>("library_cue_summary", { trackIds }),
  smartCue: (id: number) => invoke<SmartCue[]>("library_smart_cue", { id }),
  smartCueApply: (trackIds: number[], replace: boolean) =>
    invoke<SmartCueBatch>("library_smart_cue_apply", { trackIds, replace }),
  aiStatus: (trackIds: number[]) => invoke<AiStatus>("library_ai_status", { trackIds }),
  aiAnalyze: (trackIds: number[]) => invoke<number>("library_ai_analyze", { trackIds }),
  /** Own analysis (waveforms, beat grid, key) of tracks rekordbox has not analysed. */
  analyze: (trackIds: number[], again = false) => invoke<number>("library_analyze", { trackIds, again }),
  syncSelection: (stick: string) => invoke<number[]>("sync_selection", { stick }),
  syncPlan: (stick: string, playlistIds: number[]) => invoke<SyncPlan>("sync_plan", { stick, playlistIds }),
  syncStart: (stick: string, playlistIds: number[], writeSettings: boolean, engine: string | null, flat: boolean) =>
    invoke<number>("sync_start", { stick, playlistIds, writeSettings, engine, flat }),
  engineStatus: (stick: string) => invoke<EngineStatus>("engine_status", { stick }),
};

export const api = {
  listSticks: () => invoke<StickInfo[]>("list_sticks"),
  openStick: (path: string) => invoke<StickData>("open_stick", { path }),
  cueSummary: (path: string, trackIds: number[]) => invoke<CueSummary[]>("cue_summary", { path, trackIds }),
  trackDetail: (path: string, trackId: number) => invoke<TrackDetail>("track_detail", { path, trackId }),
  verifyStick: (path: string) => invoke<VerifyResult>("verify_stick", { path }),
  startExtract: (req: ExtractRequest) => invoke<number>("start_extract", { req }),
  cancelJob: (id: number) => invoke<void>("cancel_job", { id }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  eject: (path: string) => invoke<void>("eject", { path }),
  homeDir: () => invoke<string>("home_dir"),
  fullDiskAccess: () => invoke<boolean>("full_disk_access"),
  openFullDiskSettings: () => invoke<void>("open_full_disk_settings"),
  saveCues: (path: string, trackId: number, cues: Cue[]) => invoke<Cue[]>("save_cues", { path, trackId, cues }),
  revertCues: (path: string, trackId: number) => invoke<void>("revert_cues", { path, trackId }),
  pendingCues: (path: string) => invoke<number[]>("pending_cues", { path }),
  writeCues: (path: string) => invoke<CueWriteResult>("write_cues", { path }),
  cuePalette: () => invoke<PaletteColor[]>("cue_palette"),
  smartCue: (path: string, trackId: number) => invoke<SmartCue[]>("smart_cue", { path, trackId }),
  aiStatus: (path: string, trackIds: number[]) => invoke<AiStatus>("ai_status", { path, trackIds }),
  aiAnalyze: (path: string, trackIds: number[]) => invoke<number>("ai_analyze", { path, trackIds }),
  smartCueApply: (path: string, trackIds: number[], replace: boolean) =>
    invoke<SmartCueBatch>("smart_cue_apply", { path, trackIds, replace }),
};
