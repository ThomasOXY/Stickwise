<script lang="ts">
  import { Download, FolderPlus, Library, ListMusic, LoaderCircle, RefreshCw, SlidersHorizontal, Sparkles, Usb } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { Track } from "./api";
  import { keyColor, num, time } from "./format";
  import { t } from "./i18n.svelte";
  import { openSettings } from "./prefs.svelte";
  import { app } from "./state.svelte";

  const lib = $derived(app.library);
  const tracks = $derived(lib?.tracks.length ?? 0);
  const missing = $derived(lib?.tracks.filter((t) => t.missing).length ?? 0);
  const source = $derived(lib?.rekordboxSources[0]);
  const whole = $derived(app.selection?.kind !== "playlist");
  const pl = $derived(app.currentPlaylist);
  const parent = $derived(pl && pl.parentId ? app.data?.playlists.find((p) => p.id === pl.parentId) : undefined);

  // Whole library
  const hours = $derived(Math.round((lib?.tracks.reduce((s, t) => s + t.duration, 0) ?? 0) / 3600));
  const withArt = $derived(lib?.tracks.filter((t) => t.artwork).length ?? 0);
  const own = $derived(lib?.tracks.filter((t) => t.ownAnalysis).length ?? 0);

  // One playlist or set, without the search filter.
  const listTracks = $derived(
    (pl?.trackIds ?? []).map((id) => app.trackById.get(id)).filter((t): t is Track => !!t),
  );
  const length = $derived(listTracks.reduce((s, t) => s + t.duration, 0));
  const bpms = $derived(listTracks.map((t) => t.bpm).filter((b) => b > 0));
  const bpmRange = $derived.by(() => {
    if (bpms.length === 0) return "";
    const lo = Math.round(Math.min(...bpms));
    const hi = Math.round(Math.max(...bpms));
    return lo === hi ? String(lo) : `${lo}–${hi}`;
  });
  const keys = $derived(listTracks.map((t) => t.key).filter(Boolean));

  async function addFolder() {
    const d = await open({ directory: true, multiple: true });
    const paths = typeof d === "string" ? [d] : (d ?? []);
    if (paths.length) await app.addFolders(paths, null);
  }
</script>

<header class="hero" data-tauri-drag-region>
  <div class="left" data-tauri-drag-region>
    <div class="crumb">
      {#if whole}
        <span class="eyebrow kind"><Library size={12} /> {t("library.header.library")}</span>
        <span class="dim">·
          {#if app.library?.lastImport}{t("library.header.lastImport", { date: app.library.lastImport })}
          {:else if tracks > 0}{t("library.header.tracksStay")}
          {:else if source}{t("library.header.emptyImport", { source })}
          {:else}{t("library.header.noRekordbox")}{/if}
        </span>
      {:else if pl?.isSet}
        <span class="eyebrow kind"><Sparkles size={12} /> {t("library.header.set")}</span>
        <span class="dim">· {parent ? parent.name : t("library.header.yourSet")}</span>
      {:else}
        <span class="eyebrow kind"><ListMusic size={12} /> {t("library.header.playlist")}</span>
        <span class="dim">· {parent ? parent.name : pl?.source === "rekordbox" ? t("library.header.fromRekordbox") : t("library.header.ownPlaylist")}</span>
      {/if}
    </div>

    <h1>{app.currentTitle}</h1>

    <div class="stats">
      {#if whole}
        <span><b class="readout">{num(tracks)}</b> {t("library.stats.tracks", { count: tracks })}</span>
        <span><b class="readout">{num(hours)}</b> {t("library.stats.hoursOfMusic")}</span>
        <span><b class="readout">{num(withArt)}</b> {t("library.stats.withArtwork")}</span>
        <span><b class="readout">{num(own)}</b> {t("library.stats.ownAnalysis")}</span>
        {#if missing > 0}
          <span class="warn"><b class="readout">{num(missing)}</b> {t("library.stats.noFile")}</span>
        {/if}
      {:else}
        <span><b class="readout">{listTracks.length}</b> {t("library.stats.tracks", { count: listTracks.length })}</span>
        <span><b class="readout">{time(length)}</b> {t("library.stats.length")}</span>
        {#if bpmRange}<span><b class="readout">{bpmRange}</b> BPM</span>{/if}
        {#if keys.length > 1}
          <span>
            <b class="readout" style:color={keyColor(keys[0])}>{keys[0]}</b>
            <b class="readout arrow">→</b>
            <b class="readout" style:color={keyColor(keys[keys.length - 1])}>{keys[keys.length - 1]}</b>
            {t("library.stats.keys")}
          </span>
        {/if}
      {/if}
    </div>
  </div>

  <div class="actions">
    <button class="btn ghost icon" onclick={() => openSettings("player")} title={t("library.header.playerSettings")}>
      <SlidersHorizontal size={15} />
    </button>
    {#if whole}
      <button class="btn" onclick={addFolder} disabled={app.addingFolders} title={t("library.header.addFolderHint")}>
        {#if app.addingFolders}<LoaderCircle size={14} class="spin" />{:else}<FolderPlus size={14} />{/if} {t("library.header.addFolder")}
      </button>
    {/if}
    {#if source}
      <button
        class="btn"
        class:primary={whole}
        disabled={app.importing}
        onclick={() => app.importRekordbox(source)}
        title={t("library.header.importHint", { source })}
      >
        {#if app.importing}<LoaderCircle size={14} class="spin" /> {t("library.header.importing")}{:else if tracks > 0}<RefreshCw size={14} /> {t("library.header.update")}{:else}<Download size={14} /> {t("library.header.import")}{/if}
      </button>
    {/if}
    <button class="btn" class:primary={!whole || !source} onclick={() => (app.syncOpen = true)} disabled={tracks === 0}>
      <Usb size={14} /> {whole ? t("library.header.syncToStick") : t("library.header.toStick")}
    </button>
  </div>
</header>

<style>
  .hero {
    display: flex;
    align-items: flex-end;
    gap: 24px;
    padding: 24px 28px 18px;
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
    white-space: nowrap;
    overflow: hidden;
  }
  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
  }
  .dim {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  h1 {
    margin: 6px 0 8px;
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.15;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    align-items: baseline;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .stats b {
    font-size: 18px;
    color: var(--text);
    margin-right: 2px;
  }
  .stats .arrow {
    margin: 0 1px;
  }
  .warn b {
    color: var(--warning);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
