<script lang="ts">
  import { Search, Sparkles } from "@lucide/svelte";
  import { keyColor, plural, rgb, time } from "./format";
  import { app } from "./state.svelte";

  const ROW = 28;
  let smartMenu = $state(false);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let rowsEl = $state<HTMLDivElement>();
  /** Row (position) selected in the list; a track can appear twice. */
  let selIndex = $state<number | null>(null);
  let dropAt = $state<number | null>(null);

  const list = $derived(app.currentTracks);
  const aiTodo = $derived(app.currentTrackIds.filter((id) => !app.aiAnalyzed.has(id)).length);
  const total = $derived(list.reduce((s, t) => s + t.duration, 0));
  const bad = $derived(list.filter((t) => app.errorTracks.has(t.id)).length);
  const editable = $derived(app.isLibrary && app.selection?.kind === "playlist" && !app.trackFilter);

  // Only the rows in view are rendered.
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  const last = $derived(Math.min(list.length, Math.ceil((scrollTop + viewport) / ROW) + 10));
  const visible = $derived(list.slice(first, last));

  $effect(() => {
    const ids = visible.map((t) => t.id);
    const timer = setTimeout(() => app.loadCues(ids), 60);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    void app.selection;
    selIndex = null;
    if (rowsEl) rowsEl.scrollTop = 0;
  });

  function statusClass(id: number): string {
    if (app.isLibrary) {
      const t = app.trackById.get(id);
      return t?.missing ? "fault" : t?.analyzed === false ? "warn" : "ok";
    }
    if (!app.verify) return "unknown";
    const s = app.status.get(id);
    return s === "error" ? "fault" : s === "warning" ? "warn" : "ok";
  }

  function statusTitle(id: number): string {
    const t = app.trackById.get(id);
    if (!app.isLibrary || !t) return "";
    if (t.missing) return "Soubor nenalezen na Macu";
    if (t.analyzed === false) return "Bez analýzy: na CDJ bez waveformu a beatgridu";
    return "";
  }

  function choose(i: number) {
    selIndex = i;
    app.selectTrack(list[i].id);
  }

  function onKey(e: KeyboardEvent) {
    if (list.length === 0) return;
    const i = selIndex ?? list.findIndex((t) => t.id === app.selectedTrack);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const j = Math.min(list.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)));
      choose(j);
      if (rowsEl) {
        if (j * ROW < rowsEl.scrollTop) rowsEl.scrollTop = j * ROW;
        else if ((j + 1) * ROW > rowsEl.scrollTop + viewport) rowsEl.scrollTop = (j + 1) * ROW - viewport;
      }
    } else if (e.key === "Enter" && app.selectedTrack != null) {
      app.detailOpen = true;
    } else if ((e.key === "Backspace" || e.key === "Delete") && editable && i >= 0) {
      e.preventDefault();
      app.removeFromPlaylist([i]);
    }
  }

  function dragStart(e: DragEvent, i: number) {
    if (!app.isLibrary || !e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "copyMove";
    e.dataTransfer.setData("application/x-rbx-tracks", JSON.stringify([list[i].id]));
    if (editable) e.dataTransfer.setData("application/x-rbx-row", String(i));
  }

  function dragOver(e: DragEvent, i: number) {
    if (!editable || !e.dataTransfer?.types.includes("application/x-rbx-tracks")) return;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dropAt = e.clientY > r.top + r.height / 2 ? i + 1 : i;
  }

  function dropRow(e: DragEvent) {
    const at = dropAt;
    dropAt = null;
    const p = app.currentPlaylist;
    if (at == null || !p || !e.dataTransfer) return;
    e.preventDefault();
    const row = e.dataTransfer.getData("application/x-rbx-row");
    if (row !== "") {
      app.reorder(Number(row), at);
    } else {
      const ids = JSON.parse(e.dataTransfer.getData("application/x-rbx-tracks") || "[]") as number[];
      const next = [...p.trackIds];
      next.splice(at, 0, ...ids.filter((id) => !next.includes(id)));
      app.setPlaylistTracks(p.id, next);
    }
  }
</script>

<section class="table">
  <div class="title">
    <h2>{app.currentTitle}</h2>
    <span><span class="readout big">{list.length}</span> {plural(list.length, "skladba", "skladby", "skladeb")}</span>
    <span><span class="readout big">{time(total)}</span> celkem</span>
    {#if bad > 0}
      <span><span class="readout big fault">{bad}</span> {plural(bad, "nepůjde", "nepůjdou", "nepůjde")}</span>
    {/if}
    <div class="smart-wrap">
      <button class="btn small" onclick={() => (smartMenu = !smartMenu)} disabled={list.length === 0 || app.selection?.kind === "all"}>
        <Sparkles size={13} /> Smart Cue pro playlist
      </button>
      {#if smartMenu}
        <div class="smart-menu" role="menu">
          {#if app.aiInstalled && aiTodo > 0}
            <button role="menuitem" disabled={app.aiJob != null} onclick={() => ((smartMenu = false), app.aiAnalyze(app.currentTrackIds))}>
              <span>{app.aiJob != null ? "AI analýza běží…" : `Nejdřív AI analýza (${aiTodo} skladeb)`}</span>
              <span class="sub">CUE-DETR, asi 5 s na skladbu. Pak Smart Cue trefuje přesněji.</span>
            </button>
          {/if}
          <button role="menuitem" onclick={() => ((smartMenu = false), app.smartCuePlaylist(false))}>
            <span>Jen skladby bez hot cue</span>
            <span class="sub">Tvoje cue zůstanou, jak jsou.</span>
          </button>
          <button role="menuitem" onclick={() => ((smartMenu = false), app.smartCuePlaylist(true))}>
            <span>Všechny skladby, přepsat hot cue</span>
            <span class="sub">Memory cue zůstanou.</span>
          </button>
        </div>
      {/if}
    </div>
    <label class="search">
      <Search size={14} />
      <input type="search" placeholder={app.selection?.kind === "all" ? "Hledat v knihovně" : "Hledat v playlistu"} bind:value={app.trackFilter} />
    </label>
  </div>

  <div class="cols">
    <span class="c-no">#</span>
    <span class="c-st"></span>
    <span class="c-title">Název</span>
    <span class="c-artist">Interpret</span>
    <span class="c-bpm r">BPM</span>
    <span class="c-key">Tónina</span>
    <span class="c-time r">Čas</span>
    <span class="c-fmt">Formát</span>
    <span class="c-cue">Hot cue</span>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="rows"
    tabindex="0"
    role="listbox"
    aria-label="Skladby"
    bind:this={rowsEl}
    bind:clientHeight={viewport}
    onscroll={(e) => (scrollTop = (e.currentTarget as HTMLElement).scrollTop)}
    onkeydown={onKey}
    ondrop={dropRow}
    ondragleave={(e) => !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node) && (dropAt = null)}
  >
    <div style:height="{list.length * ROW}px" class="space">
      {#each visible as t, k (`${first + k}:${t.id}`)}
        {@const i = first + k}
        {@const st = statusClass(t.id)}
        {@const fmt = app.formatOverride.get(t.id) ?? t.format}
        {@const sev = app.status.get(t.id)}
        <div
          class="row"
          class:selected={selIndex === i || (selIndex == null && t.id === app.selectedTrack)}
          class:drop-before={dropAt === i}
          class:drop-after={dropAt === i + 1 && i === list.length - 1}
          class:missing={t.missing}
          style:top="{i * ROW}px"
          role="option"
          aria-selected={t.id === app.selectedTrack}
          tabindex="-1"
          draggable={app.isLibrary}
          ondragstart={(e) => dragStart(e, i)}
          ondragover={(e) => dragOver(e, i)}
          onclick={() => choose(i)}
          ondblclick={() => {
            choose(i);
            app.detailOpen = true;
          }}
          onkeydown={() => {}}
        >
          <span class="c-no readout" class:cur={t.id === app.selectedTrack}>{i + 1}</span>
          <span class="c-st" title={statusTitle(t.id)}><span class="dot {st}"></span></span>
          <span class="c-title">{t.title}{#if app.pending.has(t.id)}<i class="edited" title="Cue upravené, zatím jen v aplikaci"></i>{/if}</span>
          <span class="c-artist">{t.artist}</span>
          <span class="c-bpm r readout num">{t.bpm.toFixed(2)}</span>
          <span class="c-key"><i style:background={keyColor(t.key)}></i><span class="readout num">{t.key}</span></span>
          <span class="c-time r readout num">{time(t.duration)}</span>
          <span class="c-fmt" class:fault={sev === "error"} class:warn={sev === "warning"}>{fmt}</span>
          <span class="c-cue">
            {#each Array(8) as _, s}
              {@const c = app.cues[t.id]?.[s]}
              <i class:off={!c} style:background={c ? rgb(c, s + 1) : undefined}></i>
            {/each}
          </span>
        </div>
      {/each}
    </div>
    {#if list.length === 0}
      <p class="empty">
        {#if app.selection?.kind === "player"}Na tomhle přehrávači půjde všechno.
        {:else if app.isLibrary && app.selection?.kind === "playlist"}Prázdný. Přetáhni sem skladby z Celé knihovny nebo z jiného playlistu.
        {:else if app.isLibrary && (app.data?.tracks.length ?? 0) === 0}Knihovna je prázdná. Importuj ji z rekordboxu tlačítkem nahoře.
        {:else}Nic tu není.{/if}
      </p>
    {/if}
  </div>
</section>

<style>
  .table {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 14px;
    padding: 14px 16px 12px;
    color: var(--text-2);
    font-size: 12px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .big {
    font-size: 16px;
    color: var(--text);
  }
  .big.fault {
    color: var(--fault);
  }
  .smart-wrap {
    position: relative;
    margin-left: auto;
    align-self: center;
  }
  .btn.small {
    height: 28px;
    padding: 0 10px;
    font-size: 12px;
  }
  .smart-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 6px);
    width: 280px;
    padding: 6px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
    z-index: 25;
  }
  .smart-menu button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    color: var(--text);
    font-size: 13px;
  }
  .smart-menu button:hover {
    background: var(--panel-2);
  }
  .smart-menu .sub {
    font-size: 11px;
    color: var(--text-2);
  }
  .search {
    align-self: center;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 200px;
    padding: 0 10px;
    height: 28px;
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--text-2);
  }
  .search:focus-within {
    border-color: var(--cue);
  }
  .search input {
    border: 0;
    height: 26px;
    padding: 0;
    flex: 1;
    min-width: 0;
    background: transparent;
  }
  .cols,
  .row {
    display: grid;
    grid-template-columns: 24px 16px minmax(160px, 1fr) minmax(120px, 0.62fr) 52px 56px 40px 84px 92px;
    gap: 8px;
    align-items: center;
    padding: 0 16px;
  }
  .cols {
    height: 30px;
    font-size: 11px;
    color: var(--text-2);
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .rows {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    outline: none;
  }
  .space {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: 28px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
  }
  .row:hover {
    background: rgb(255 255 255 / 0.025);
  }
  .row.selected {
    background: var(--panel-2);
  }
  .row.missing .c-title,
  .row.missing .c-artist {
    color: var(--text-3);
  }
  .row.drop-before {
    box-shadow: inset 0 2px 0 var(--cue);
  }
  .row.drop-after {
    box-shadow: inset 0 -2px 0 var(--cue);
  }
  .r {
    text-align: right;
  }
  .c-no {
    text-align: right;
    color: var(--text-2);
  }
  .c-no.cur {
    color: var(--cue);
  }
  .c-st {
    display: flex;
    justify-content: center;
  }
  .c-title {
    font-weight: 500;
  }
  .c-title,
  .c-artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-artist {
    color: var(--text-2);
  }
  .edited {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 7px;
    border-radius: 50%;
    background: var(--cue);
    vertical-align: middle;
  }
  .num {
    font-size: 15px;
  }
  .c-key {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 12px;
  }
  .c-key i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: none;
  }
  .c-fmt {
    padding-left: 16px;
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .c-fmt.fault {
    color: var(--fault);
  }
  .c-fmt.warn {
    color: var(--warn);
  }
  .c-cue {
    display: flex;
    gap: 3px;
  }
  .c-cue i {
    width: 7px;
    height: 7px;
    border-radius: 1px;
  }
  .c-cue i.off {
    background: var(--line);
  }
  .empty {
    padding: 24px 16px;
    color: var(--text-2);
  }
</style>
