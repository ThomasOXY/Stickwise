<script lang="ts">
  import { Download, Eject, LibraryBig } from "@lucide/svelte";
  import { api } from "./api";
  import { gb } from "./format";
  import { app } from "./state.svelte";

  function stickStatus(path: string): string {
    if (path !== app.stickPath || !app.verify) return "unknown";
    if (app.errorTracks.size > 0 || app.verify.players.some((p) => !p.filesystemOk)) return "fault";
    return app.verify.issues.length > 0 ? "warn" : "ok";
  }

  async function eject(e: MouseEvent, path: string) {
    e.stopPropagation();
    try {
      await api.eject(path);
      await app.refreshSticks();
    } catch (err) {
      app.error = `Flashku nejde vysunout: ${err}`;
    }
  }

  const running = $derived(app.jobs.filter((j) => j.state === "running"));
</script>

<aside class="sidebar">
  <div class="drag" data-tauri-drag-region></div>

  <section>
    <h2>Flashky</h2>
    {#if app.sticks.length === 0}
      <p class="empty">Žádná připojená flashka</p>
    {/if}
    {#each app.sticks as s (s.path)}
      {@const used = s.totalBytes - s.freeBytes}
      <div
        class="stick"
        class:active={s.path === app.stickPath}
        role="button"
        tabindex="0"
        onclick={() => app.open(s.path)}
        onkeydown={(e) => e.key === "Enter" && app.open(s.path)}
      >
        <div class="row">
          <span class="dot {stickStatus(s.path)}"></span>
          <span class="name">{s.name}</span>
          <button class="eject" title="Vysunout {s.name}" onclick={(e) => eject(e, s.path)}>
            <Eject size={12} />
          </button>
        </div>
        <div class="cap">
          <div class="bar"><div style:width="{(used / Math.max(1, s.totalBytes)) * 100}%"></div></div>
          <span class="readout">{gb(used)} / {gb(s.totalBytes)} GB</span>
        </div>
      </div>
    {/each}
  </section>

  <section>
    <h2>Knihovna <span class="badge">Brzy</span></h2>
    <div class="soon"><LibraryBig size={14} /> Celá knihovna</div>
  </section>

  <section class="jobs">
    <h2>Úlohy</h2>
    {#if running.length === 0}
      <p class="empty">Nic neběží</p>
    {/if}
    {#each running as j (j.id)}
      <div class="job">
        <div class="row">
          <Download size={13} />
          <span class="label">{j.label}</span>
          <span class="readout count">{j.filesDone} / {j.filesTotal}</span>
        </div>
        <div class="bar amber"><div style:width="{(j.bytesDone / Math.max(1, j.bytesTotal)) * 100}%"></div></div>
      </div>
    {/each}
  </section>
</aside>

<style>
  .sidebar {
    width: 240px;
    flex: none;
    display: flex;
    flex-direction: column;
    padding: 0 10px 18px;
    background: rgb(28 31 38 / 0.72);
    border-right: 1px solid rgb(0 0 0 / 0.35);
  }
  .drag {
    height: 52px;
    flex: none;
  }
  section {
    margin-bottom: 22px;
  }
  .jobs {
    margin-top: auto;
    margin-bottom: 0;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 10px 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .badge {
    font-weight: 500;
    color: var(--text-3);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 1px 6px;
  }
  .empty {
    margin: 0 10px;
    color: var(--text-2);
  }
  .stick {
    padding: 8px 10px;
    border-radius: 6px;
    margin-bottom: 2px;
  }
  .stick:hover {
    background: rgb(255 255 255 / 0.03);
  }
  .stick.active {
    background: var(--panel-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    flex: 1;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .eject {
    border: 0;
    background: none;
    color: var(--text-3);
    padding: 2px;
    border-radius: 4px;
    opacity: 0;
  }
  .stick:hover .eject,
  .eject:focus-visible {
    opacity: 1;
  }
  .eject:hover {
    color: var(--text);
  }
  .cap {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 0 16px;
  }
  .cap .readout {
    font-size: 11px;
    color: var(--text-2);
  }
  .bar {
    flex: 1;
    height: 3px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--text-2);
  }
  .bar.amber > div {
    background: var(--cue);
  }
  .soon {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 10px;
    color: var(--text-3);
  }
  .job {
    padding: 0 10px;
    margin-bottom: 10px;
  }
  .job .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .job .count {
    font-size: 12px;
    color: var(--text-2);
  }
  .job .bar {
    margin-top: 8px;
  }
</style>
