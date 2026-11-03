import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

/** The update found, still to download, downloading, or installed and waiting for the restart. */
type Phase = "idle" | "checking" | "available" | "downloading" | "ready" | "current" | "error";

/** Notes of the version being installed, shown once after the restart. */
const NOTES_KEY = "update-notes";
const SKIPPED_KEY = "update-skipped";
/** How often a running app looks again. */
const EVERY_MS = 4 * 60 * 60 * 1000;

class Updates {
  phase = $state<Phase>("idle");
  current = $state("");
  update = $state<Update | null>(null);
  error = $state<string | null>(null);
  /** Bytes downloaded and total; total 0 while the server has not said. */
  got = $state(0);
  total = $state(0);
  open = $state(false);
  /** Notes of the version just installed: shown once, after the restart. */
  installed = $state<{ version: string; notes: string } | null>(null);

  available = $derived(this.update !== null && (this.phase === "available" || this.phase === "downloading" || this.phase === "ready"));

  async init() {
    this.current = await getVersion();
    const saved = localStorage.getItem(NOTES_KEY);
    if (saved) {
      const n = JSON.parse(saved) as { version: string; notes: string };
      localStorage.removeItem(NOTES_KEY);
      if (n.version === this.current) {
        this.installed = n;
        this.open = true;
      }
    }
    // Let the library load first; the check is not urgent.
    setTimeout(() => this.check(true), 5000);
    setInterval(() => this.check(true), EVERY_MS);
  }

  /** `quiet`: the background check; opens the sheet only for a version not skipped. */
  async check(quiet = false) {
    if (this.phase === "checking" || this.phase === "downloading" || this.phase === "ready") return;
    this.phase = "checking";
    this.error = null;
    try {
      const u = await check();
      this.update = u;
      this.phase = u ? "available" : "current";
      if (u && quiet && localStorage.getItem(SKIPPED_KEY) !== u.version) this.open = true;
    } catch (e) {
      this.phase = "error";
      this.error = String(e);
    }
  }

  async install() {
    const u = this.update;
    if (!u || this.phase !== "available") return;
    this.phase = "downloading";
    this.got = 0;
    this.total = 0;
    try {
      await u.downloadAndInstall((ev) => {
        if (ev.event === "Started") this.total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") this.got += ev.data.chunkLength;
      });
      localStorage.setItem(NOTES_KEY, JSON.stringify({ version: u.version, notes: u.body ?? "" }));
      this.phase = "ready";
    } catch (e) {
      this.phase = "available";
      this.error = String(e);
    }
  }

  restart() {
    return relaunch();
  }

  /** Later: the background check stays quiet about this version. */
  skip() {
    if (this.update) localStorage.setItem(SKIPPED_KEY, this.update.version);
    this.open = false;
  }

  close() {
    this.open = false;
    this.installed = null;
  }
}

export const updates = new Updates();
