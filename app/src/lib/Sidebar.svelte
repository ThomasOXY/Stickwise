<script lang="ts">
  import { Download, Eject, Laptop, Usb } from "@lucide/svelte";
  import PlaylistTree from "./PlaylistTree.svelte";
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

  <div class="head" data-tauri-drag-region>
    <h1>{app.isLibrary ? "Knihovna" : app.data?.info.name ?? "Flashka"}</h1>
    <button class="fmt" title="Smazat a připravit flashku pro přehrávače" onclick={() => (app.formatOpen = true)}>
      Formátovat…
    </button>
  </div>

  <section class="sources">
    <div
      class="src"
      class:active={app.isLibrary}
      role="button"
      tabindex="0"
      onclick={() => !app.isLibrary && app.openLibrary()}
      onkeydown={(e) => e.key === "Enter" && app.openLibrary()}
    >
      <div class="row">
        <Laptop size={16} />
        <span class="name">Tento Mac</span>
        <span class="readout count">{app.library?.tracks.length ?? 0}</span>
      </div>
    </div>
    {#each app.sticks as s (s.path)}
      {@const used = s.totalBytes - s.freeBytes}
      <div
        class="src"
        class:active={!app.isLibrary && s.path === app.stickPath}
        role="button"
        tabindex="0"
        onclick={() => app.open(s.path)}
        onkeydown={(e) => e.key === "Enter" && app.open(s.path)}
      >
        <div class="row">
          <Usb size={16} />
          <span class="name">{s.name}</span>
          <span class="dot {stickStatus(s.path)}"></span>
          <span class="readout count">{gb(used)} / {gb(s.totalBytes)} GB</span>
          <button class="eject" title="Vysunout {s.name}" onclick={(e) => eject(e, s.path)}>
            <Eject size={12} />
          </button>
        </div>
        <div class="bar"><div style:width="{(used / Math.max(1, s.totalBytes)) * 100}%"></div></div>
      </div>
    {/each}
    {#if app.sticks.length === 0}
      <p class="empty">Žádná připojená flashka</p>
    {/if}
  </section>

  {#if app.data}<PlaylistTree />{:else}<div class="fill"></div>{/if}

  {#if running.length > 0}
    <section class="jobs">
      {#each running as j (j.id)}
        <div class="job">
          <div class="row">
            <Download size={13} />
            <span class="label">{j.label}</span>
            <span class="readout count">{j.filesDone} / {j.filesTotal}</span>
          </div>
          <div class="bar">
            <div style:width="{(j.fraction ?? j.bytesDone / Math.max(1, j.bytesTotal)) * 100}%"></div>
          </div>
        </div>
      {/each}
    </section>
  {/if}
</aside>

<style>
  .sidebar {
    width: 264px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 0 6px 12px 0;
  }
  .drag {
    height: 30px;
    flex: none;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px 14px;
  }
  h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fmt {
    border: 0;
    background: none;
    padding: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .fmt:hover {
    color: var(--text);
  }
  .sources {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 18px;
  }
  .src {
    padding: 10px;
    border-radius: 10px;
    color: var(--text-2);
  }
  .src:hover {
    background: rgb(255 255 255 / 0.03);
  }
  .src.active {
    background: var(--surface-2);
    color: var(--text);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  .src.active .row :global(svg) {
    color: var(--text);
  }
  .name {
    flex: 1;
    font-size: 13.5px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    font-size: 12px;
    color: var(--text-3);
  }
  .eject {
    display: none;
    border: 0;
    background: none;
    color: var(--text-3);
    padding: 2px;
    border-radius: 4px;
  }
  .src:hover .eject,
  .eject:focus-visible {
    display: block;
  }
  .src:hover .count {
    display: none;
  }
  .eject:hover {
    color: var(--text);
  }
  .bar {
    height: 4px;
    margin-top: 8px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
  }
  .empty {
    margin: 4px 10px;
    font-size: 12px;
    color: var(--text-3);
  }
  .fill {
    flex: 1;
  }
  .jobs {
    padding: 12px 10px 0;
    border-top: 1px solid var(--hairline);
  }
  .job + .job {
    margin-top: 10px;
  }
  .job .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
