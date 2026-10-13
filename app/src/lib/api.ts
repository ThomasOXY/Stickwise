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
}

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
