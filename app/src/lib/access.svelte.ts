import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "./api";

/** How often an open sheet looks whether the DJ has switched access on. */
const POLL_MS = 1500;

/**
 * Full Disk Access. With it macOS stops asking about the music on external
 * disks, the rekordbox library and USB sticks; without it every read may ask
 * again. Only the DJ can switch it on, so the app shows where.
 */
class Access {
  granted = $state(true);
  open = $state(false);
  private poll: ReturnType<typeof setInterval> | undefined;

  async init() {
    this.granted = await api.fullDiskAccess();
    if (!this.granted) this.show();
  }

  show() {
    this.open = true;
    this.poll ??= setInterval(async () => {
      if (await api.fullDiskAccess()) {
        this.granted = true;
        this.stop();
      }
    }, POLL_MS);
  }

  settings() {
    return api.openFullDiskSettings();
  }

  restart() {
    return relaunch();
  }

  /** Later: asks again at the next start. */
  close() {
    this.stop();
    this.open = false;
  }

  private stop() {
    clearInterval(this.poll);
    this.poll = undefined;
  }
}

export const access = new Access();
