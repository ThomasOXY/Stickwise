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
  import { fsName, gb, num } from "./format";
  import { i18n, t } from "./i18n.svelte";
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
      app.error = t("sync.ejectFailed", { error: String(e) });
    }
  }

  function check() {
    app.view = "health";
    if (!app.verify) app.runVerify();
  }

  /** GB with one decimal in the app's language. */
  const gbText = (bytes: number) =>
    Number(gb(bytes)).toLocaleString(i18n.lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const checking = $derived(t("sync.header.checking", { progress: "\u0000" }).split("\u0000"));
</script>

<header class="hero" data-tauri-drag-region>
  <div class="top" data-tauri-drag-region>
    <div class="left" data-tauri-drag-region>
      <div class="crumb">
        <span class="eyebrow kind"><Usb size={12} /> {t("sync.stick")}</span>
        <span>{t("sync.header.mounted", { fs: fsName(info.filesystem) })}</span>
      </div>
      <h1>{info.name}</h1>
      <div class="cap">
        <div class="bar"><div style:width="{(used / Math.max(1, info.totalBytes)) * 100}%"></div></div>
        <span><b class="readout">{t("sync.header.capacity", { used: gbText(used), total: gbText(info.totalBytes) })}</b></span>
        <span class="meta">· {t("common.tracks", { count: num(trackCount) })} · {t("sync.playlists", { count: num(listCount) })}</span>
      </div>
    </div>

    <div class="actions">
      <button class="btn ghost icon" title={t("sync.header.formatTitle")} onclick={() => (app.formatOpen = true)}>
        <Eraser size={15} />
      </button>
      <button class="btn ghost icon" disabled title={t("sync.header.clone")}><Copy size={15} /></button>
      <button class="btn ghost" title={t("sync.header.eject")} onclick={eject}><Eject size={15} /> {t("sync.header.eject")}</button>
      <button class="btn" onclick={() => (app.settingsFor = info.path)} title={t("sync.header.settingsTitle")}>
        <SlidersHorizontal size={14} /> {t("player.title")}
      </button>
      <button class="btn" onclick={() => (app.exportOpen = true)}><HardDriveDownload size={14} /> {t("sync.header.export")}</button>
      {#if app.pending.size > 0}
        <button class="btn write" onclick={() => (app.writeOpen = true)}>
          <Save size={14} /> {t("sync.header.writeCues")} <span class="readout">{app.pending.size}</span>
        </button>
      {/if}
      {#if app.view === "health"}
        <button class="btn primary" onclick={() => app.runVerify()} disabled={app.verifying}>
          <RefreshCw size={14} /> {t("common.checkAgain")}
        </button>
      {:else}
        <button class="btn primary" onclick={check}><ShieldCheck size={14} /> {t("sync.header.check")}</button>
      {/if}
    </div>
  </div>

  <div class="bottom">
    <div class="seg">
      <button class:on={app.view !== "health"} onclick={() => (app.view = "tracks")}>
        {t("sync.header.tracks")} <span class="n readout">{trackCount}</span>
      </button>
      <button class:on={app.view === "health"} onclick={check}>
        {t("sync.header.checkTab")} {#if app.verify}<span class="n readout" class:bad={issues > 0}>{issues}</span>{/if}
      </button>
    </div>

    <div class="players">
      <span class="eyebrow">{t("sync.players")}</span>
      {#each PLAYERS as p}
        {@const active = app.selection?.kind === "player" && app.selection.player === p}
        <button
          class="chip"
          class:active
          disabled={!app.verify}
          title={app.verify ? t("sync.header.playerTitle") : t("sync.header.checkFirst")}
          onclick={() => (active ? (app.view = "tracks") : app.select({ kind: "player", player: p }))}
        >
          <span class="dot {playerStatus(p)}"></span>{p.replace("CDJ-", "")}
        </button>
      {/each}
    </div>

    {#if app.verifying}
      <div class="pill"><LoaderCircle size={12} class="spin" /> {checking[0]}<span class="readout">{app.verifyProgress[0]} / {app.verifyProgress[1]}</span>{checking[1]}</div>
    {:else if app.verify}
      {#if errors > 0}
        <button class="pill fault" onclick={check}>
          <span class="dot fault"></span>
          {t("sync.header.unplayable", { count: num(errors) })}
        </button>
      {:else}
        <button class="pill ok" onclick={check}><span class="dot ok"></span> {t("sync.header.allPlayable")}</button>
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
