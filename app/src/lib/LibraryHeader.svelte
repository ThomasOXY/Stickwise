<script lang="ts">
  import { Download, LoaderCircle, RefreshCw, SlidersHorizontal } from "@lucide/svelte";
  import { plural } from "./format";
  import { app } from "./state.svelte";

  const lib = $derived(app.library);
  const tracks = $derived(lib?.tracks.length ?? 0);
  const sets = $derived(lib?.playlists.filter((p) => p.isSet).length ?? 0);
  const lists = $derived(lib?.playlists.filter((p) => !p.isFolder && !p.isSet).length ?? 0);
  const missing = $derived(lib?.tracks.filter((t) => t.missing).length ?? 0);
  const source = $derived(lib?.rekordboxSources[0]);
</script>

<header data-tauri-drag-region>
  <div class="top" data-tauri-drag-region>
    <h1 class="readout">KNIHOVNA</h1>
    <div class="stats">
      <span><span class="readout n">{tracks}</span> {plural(tracks, "skladba", "skladby", "skladeb")}</span>
      <span><span class="readout n">{sets}</span> {plural(sets, "set", "sety", "setů")}</span>
      <span><span class="readout n">{lists}</span> {plural(lists, "playlist", "playlisty", "playlistů")}</span>
      {#if missing > 0}
        <span class="warn"><span class="readout n">{missing}</span> bez souboru</span>
      {/if}
    </div>

    <div class="actions">
      {#if source}
        <button
          class="btn"
          disabled={app.importing}
          onclick={() => app.importRekordbox(source)}
          title="Načte skladby, playlisty a cue z rekordboxu ({source}). Tvoje sety a úpravy zůstanou."
        >
          {#if app.importing}<LoaderCircle size={14} class="spin" /> Načítám z rekordboxu…{:else if tracks > 0}<RefreshCw size={14} /> Aktualizovat z rekordboxu{:else}<Download size={14} /> Importovat z rekordboxu{/if}
        </button>
      {/if}
      <button class="btn" onclick={() => (app.settingsFor = null)} title="Tvoje My Settings pro CDJ/XDJ a mixer">
        <SlidersHorizontal size={14} /> Nastavení přehrávače
      </button>
      <button class="btn primary" onclick={() => (app.syncOpen = true)} disabled={tracks === 0}>
        <RefreshCw size={14} /> Synchronizovat na flashku
      </button>
    </div>
  </div>
  <div class="sub">
    {#if app.library?.lastImport}
      Naposledy z rekordboxu: {app.library.lastImport}
    {:else if source}
      Knihovna je prázdná. Importuj ji z rekordboxu ({source}), skladby zůstanou, kde jsou.
    {:else}
      Rekordbox knihovna nenalezena.
    {/if}
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
    gap: 24px;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    letter-spacing: 0.02em;
    font-weight: 600;
  }
  .stats {
    display: flex;
    gap: 16px;
    font-size: 12px;
    color: var(--text-2);
    align-items: baseline;
  }
  .n {
    font-size: 17px;
    color: var(--text);
  }
  .warn .n {
    color: var(--warn);
  }
  .actions {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }
  .sub {
    margin-top: 8px;
    font-size: 11px;
    color: var(--text-2);
  }
</style>
