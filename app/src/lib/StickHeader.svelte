<script lang="ts">
  import {
    Copy,
    Eject,
    Eraser,
    HardDriveDownload,
    LoaderCircle,
    RefreshCw,
    Save,
    ShieldCheck,
    SlidersHorizontal,
    Usb,
  } from "@lucide/svelte";
  import { api } from "./api";
  import { fsName, gb, plural } from "./format";
  import { app } from "./state.svelte";

  const info = $derived(app.data!.info);
  const used = $derived(info.totalBytes - info.freeBytes);
  const errors = $derived(app.errorTracks.size);
  const trackCount = $derived(app.data?.tracks.length ?? 0);
  const listCount = $derived(app.data?.playlists.filter((p) => !p.isFolder).length ?? 0);
  const issues = $derived(app.verify?.issues.length ?? 0);

  function playerStatus(name: string): string {
    const p = app.verify?.players.find((p) => p.player === name);
    if (!p) return "unknown";
    if (!p.filesystemOk) return "fault";
    return p.unplayable.length > 0 ? "warn" : "ok";
  }

  const PLAYERS = ["CDJ-2000NXS", "CDJ-2000NXS2", "CDJ-3000", "CDJ-3000X"];

  async function eject() {
    try {
      await api.eject(info.path);
      await app.refreshSticks();
    } catch (e) {
      app.error = `Flashku nejde vysunout: ${e}`;
    }
  }

  function check() {
    app.view = "health";
    if (!app.verify) app.runVerify();
  }
</script>

<header class="hero" data-tauri-drag-region>
  <div class="top" data-tauri-drag-region>
    <div class="left" data-tauri-drag-region>
      <div class="crumb">
        <span class="eyebrow kind"><Usb size={12} /> Flashka</span>
        <span>· připojená · {fsName(info.filesystem)}</span>
      </div>
      <h1>{info.name}</h1>
      <div class="cap">
        <div class="bar"><div style:width="{(used / Math.max(1, info.totalBytes)) * 100}%"></div></div>
        <span><b class="readout">{gb(used).replace(".", ",")} z {gb(info.totalBytes).replace(".", ",")} GB</b></span>
        <span class="meta">· {trackCount} {plural(trackCount, "skladba", "skladby", "skladeb")} · {listCount} {plural(listCount, "playlist", "playlisty", "playlistů")}</span>
      </div>
    </div>

    <div class="actions">
      <button class="btn ghost icon" title="Formátovat flashku (FAT32 / exFAT)" onclick={() => (app.formatOpen = true)}>
        <Eraser size={15} />
      </button>
      <button class="btn ghost icon" disabled title="Klonovat (brzy)"><Copy size={15} /></button>
      <button class="btn ghost" title="Vysunout" onclick={eject}><Eject size={15} /> Vysunout</button>
      <button class="btn" onclick={() => (app.settingsFor = info.path)} title="My Settings pro CDJ/XDJ a mixer na této flashce">
        <SlidersHorizontal size={14} /> Nastavení přehrávače
      </button>
      <button class="btn" onclick={() => (app.exportOpen = true)}><HardDriveDownload size={14} /> Export na disk</button>
      {#if app.pending.size > 0}
        <button class="btn write" onclick={() => (app.writeOpen = true)}>
          <Save size={14} /> Zapsat cue na flashku <span class="readout">{app.pending.size}</span>
        </button>
      {/if}
      {#if app.view === "health"}
        <button class="btn primary" onclick={() => app.runVerify()} disabled={app.verifying}>
          <RefreshCw size={14} /> Zkontrolovat znovu
        </button>
      {:else}
        <button class="btn primary" onclick={check}><ShieldCheck size={14} /> Zkontrolovat flashku</button>
      {/if}
    </div>
  </div>

  <div class="bottom">
    <div class="seg">
      <button class:on={app.view !== "health"} onclick={() => (app.view = "tracks")}>
        Skladby <span class="n readout">{trackCount}</span>
      </button>
      <button class:on={app.view === "health"} onclick={check}>
        Kontrola {#if app.verify}<span class="n readout" class:bad={issues > 0}>{issues}</span>{/if}
      </button>
    </div>

    <div class="players">
      <span class="eyebrow">Přehrávače</span>
      {#each PLAYERS as p}
        {@const active = app.selection?.kind === "player" && app.selection.player === p}
        <button
          class="chip"
          class:active
          disabled={!app.verify}
          title={app.verify ? "Skladby, které na tomto přehrávači nepůjdou" : "Nejdřív zkontroluj flashku"}
          onclick={() => (active ? (app.view = "tracks") : app.select({ kind: "player", player: p }))}
        >
          <span class="dot {playerStatus(p)}"></span>{p.replace("CDJ-", "")}
        </button>
      {/each}
    </div>

    {#if app.verifying}
      <div class="pill"><LoaderCircle size={12} class="spin" /> Kontroluji <span class="readout">{app.verifyProgress[0]} / {app.verifyProgress[1]}</span></div>
    {:else if app.verify}
      {#if errors > 0}
        <button class="pill fault" onclick={check}>
          <span class="dot fault"></span>
          {errors} {plural(errors, "skladba nepůjde", "skladby nepůjdou", "skladeb nepůjde")} přehrát
        </button>
      {:else}
        <button class="pill ok" onclick={check}><span class="dot ok"></span> Všechno půjde přehrát</button>
      {/if}
    {/if}
  </div>
</header>

<style>
  .hero {
    padding: 24px 28px 14px;
  }
  .top {
    display: flex;
    align-items: flex-end;
    gap: 24px;
  }
  .left {
    flex: 1;
    min-width: 0;
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-3);
  }
  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
  }
  h1 {
    margin: 6px 0 10px;
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.15;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cap {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12.5px;
    white-space: nowrap;
  }
  .bar {
    width: 220px;
    height: 5px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
    flex: none;
  }
  .bar > div {
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .cap b {
    font-size: 16px;
  }
  .meta {
    color: var(--text-3);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
  .btn.write {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .btn.write :global(svg) {
    color: var(--accent);
  }
  .btn.write .readout {
    font-size: 15px;
  }
  .bottom {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-top: 18px;
    flex-wrap: wrap;
  }
  .seg .n {
    margin-left: 6px;
    font-size: 12px;
    color: var(--text-3);
  }
  .seg .n.bad {
    color: var(--danger);
  }
  .players {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .players .eyebrow {
    margin-right: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 26px;
    padding: 0 11px;
    border-radius: 13px;
    border: 0;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: 12px;
    font-weight: 500;
  }
  .chip:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--text);
  }
  .chip.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip:disabled {
    opacity: 0.55;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 12px;
    border-radius: 13px;
    border: 0;
    background: var(--surface-2);
    color: var(--text-2);
    font-size: 12px;
  }
  .pill .readout {
    font-size: 13px;
  }
  .pill.fault {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .pill.ok {
    background: var(--success-soft);
    color: var(--success);
  }
</style>
