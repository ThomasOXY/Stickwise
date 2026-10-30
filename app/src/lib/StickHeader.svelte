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
  } from "@lucide/svelte";
  import { api } from "./api";
  import { fsName, gb, plural } from "./format";
  import { app } from "./state.svelte";

  const info = $derived(app.data!.info);
  const used = $derived(info.totalBytes - info.freeBytes);
  const errors = $derived(app.errorTracks.size);

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

<header data-tauri-drag-region>
  <div class="top" data-tauri-drag-region>
    <h1 class="readout">{info.name}</h1>

    <div class="cap">
      <div class="bar"><div style:width="{(used / Math.max(1, info.totalBytes)) * 100}%"></div></div>
      <div><span class="readout num">{gb(used)}</span> <span class="sub">z {gb(info.totalBytes)} GB obsazeno</span></div>
    </div>

    <div class="fs">
      <div>{fsName(info.filesystem)}</div>
      <div class="sub">Souborový systém</div>
    </div>

    {#if app.verifying}
      <div class="pill"><LoaderCircle size={12} class="spin" /> Kontroluji {app.verifyProgress[0]} / {app.verifyProgress[1]}</div>
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

    <div class="actions">
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
      <button class="btn" onclick={() => (app.exportOpen = true)}><HardDriveDownload size={14} /> Exportovat na disk</button>
      <button class="btn" onclick={() => (app.settingsFor = info.path)} title="My Settings pro CDJ/XDJ a mixer na této flashce">
        <SlidersHorizontal size={14} /> Nastavení přehrávače
      </button>
      <button class="btn" disabled title="Brzy"><Copy size={14} /> Klonovat</button>
      <button class="btn icon" title="Formátovat flashku (FAT32 / exFAT)" onclick={() => (app.formatOpen = true)}>
        <Eraser size={14} />
      </button>
      <button class="btn icon" title="Vysunout" onclick={eject}><Eject size={14} /></button>
    </div>
  </div>

  <div class="players">
    <span class="label">Přehrávače</span>
    {#each PLAYERS as p}
      {@const active = app.selection?.kind === "player" && app.selection.player === p}
      <button
        class="chip"
        class:active
        disabled={!app.verify}
        onclick={() => (active ? app.view = "tracks" : app.select({ kind: "player", player: p }))}
      >
        <span class="dot {playerStatus(p)}"></span>{p}
      </button>
    {/each}
    <span class="hint">Klikni na přehrávač a uvidíš skladby, které na něm nepůjdou.</span>
  </div>
</header>

<style>
  header {
    padding: 18px 24px 12px;
    border-bottom: 1px solid var(--line);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    letter-spacing: 0.02em;
    font-weight: 600;
  }
  .cap {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 132px;
  }
  .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--text);
  }
  .num {
    font-size: 14px;
  }
  .sub {
    font-size: 11px;
    color: var(--text-2);
  }
  .fs {
    line-height: 1.35;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 12px;
    border-radius: 13px;
    border: 1px solid var(--line);
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
  }
  .pill.fault {
    border-color: rgb(229 72 77 / 0.45);
    background: rgb(229 72 77 / 0.1);
    color: #f2a3a5;
  }
  .pill.ok {
    border-color: rgb(89 201 138 / 0.4);
    color: var(--play);
  }
  .actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }
  .btn.write {
    border-color: var(--cue);
    color: var(--cue);
  }
  .btn.write .readout {
    font-size: 14px;
  }
  .players {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
    margin-right: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 10px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--panel);
    font-size: 12px;
    font-weight: 500;
  }
  .chip:hover:not(:disabled) {
    background: var(--panel-2);
  }
  .chip.active {
    border-color: var(--cue);
  }
  .chip:disabled {
    opacity: 0.6;
  }
  .hint {
    margin-left: 6px;
    font-size: 11px;
    color: var(--text-2);
  }
</style>
