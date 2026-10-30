<script lang="ts">
  import { ArrowDown, ArrowUp, ArrowUpDown, AudioWaveform, ChevronRight, ListMusic, ListPlus, Search, Sparkles, Trash } from "@lucide/svelte";
  import type { Track } from "./api";
  import { date, keyColor, keyOrder, plural, rgb, time } from "./format";
  import { app } from "./state.svelte";

  const ROW = 28;
  let smartMenu = $state(false);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let rowsEl = $state<HTMLDivElement>();
  /** Selected rows (positions: a track can appear twice in a playlist). */
  let selRows = $state<Set<number>>(new Set());
  /** Where a shift-click range starts. */
  let anchor = $state<number | null>(null);
  let dropAt = $state<number | null>(null);
  let menu = $state<{ x: number; y: number; sub: "sets" | "playlists" | null } | null>(null);
  let menuFilter = $state("");

  /** Track ids of the selected rows, in list order, without repeats. */
  const selectedIds = $derived([...new Set([...selRows].sort((a, b) => a - b).map((i) => list[i]?.id).filter((id) => id != null))]);
  const sets = $derived((app.data?.playlists ?? []).filter((p) => p.isSet));
  const plainLists = $derived((app.data?.playlists ?? []).filter((p) => !p.isSet && !p.isFolder));

  type SortKey = "title" | "artist" | "bpm" | "key" | "time" | "format" | "added";
  /** Column the table is sorted by; none = the playlist's own order. Kept between playlists. */
  let sort = $state<{ key: SortKey; desc: boolean } | null>(JSON.parse(localStorage.getItem("track-sort") ?? "null"));
  $effect(() => localStorage.setItem("track-sort", JSON.stringify(sort)));

  const coll = new Intl.Collator("cs", { numeric: true, sensitivity: "base" });
  /** Value to compare by; `null` (empty) always sorts last. */
  const value: Record<SortKey, (t: Track) => string | number | null> = {
    title: (t) => t.title || null,
    artist: (t) => t.artist || null,
    bpm: (t) => t.bpm || null,
    key: (t) => (Number.isFinite(keyOrder(t.key)) ? keyOrder(t.key) : null),
    time: (t) => t.duration || null,
    format: (t) => app.formatOverride.get(t.id) ?? (t.format || null),
    added: (t) => t.dateAdded || null,
  };

  /** Click: ascending, again descending, a third time back to the playlist order. */
  function sortBy(key: SortKey) {
    sort = sort?.key !== key ? { key, desc: false } : sort.desc ? null : { key, desc: true };
  }

  /** Row → position in the unsorted list (a track can be in a playlist twice). */
  const order = $derived.by(() => {
    const tracks = app.currentTracks;
    const idx = tracks.map((_, i) => i);
    if (!sort) return idx;
    const { key, desc } = sort;
    const vals = tracks.map(value[key]);
    // Stable: equal values keep the playlist order.
    return idx.sort((a, b) => {
      const va = vals[a];
      const vb = vals[b];
      if (va === null || vb === null) return va === vb ? a - b : va === null ? 1 : -1;
      const c = typeof va === "number" && typeof vb === "number" ? va - vb : coll.compare(String(va), String(vb));
      return (desc ? -c : c) || a - b;
    });
  });
  const list = $derived(order.map((i) => app.currentTracks[i]));

  /** Make the sorted order the playlist's own. */
  async function keepOrder() {
    const p = app.currentPlaylist;
    if (!p || app.trackFilter) return;
    await app.setPlaylistTracks(p.id, list.map((t) => t.id));
    sort = null;
    app.notice = `Pořadí v „${p.name}“ uloženo.`;
  }
  const aiTodo = $derived(app.currentTrackIds.filter((id) => !app.aiAnalyzed.has(id)).length);
  const total = $derived(list.reduce((s, t) => s + t.duration, 0));
  const toAnalyze = $derived(app.isLibrary ? app.unanalyzed(app.currentTrackIds).length : 0);
  const job = $derived(app.jobs.find((j) => j.id === app.analyzeJob));
  const bad = $derived(list.filter((t) => app.errorTracks.has(t.id)).length);
  // Rows map to playlist positions only unfiltered; dragging rows into place only in its own order.
  const editable = $derived(app.isLibrary && app.selection?.kind === "playlist" && !app.trackFilter);
  const reorderable = $derived(editable && !sort);
  const canKeep = $derived(!!sort && app.isLibrary && !!app.currentPlaylist && !app.currentPlaylist.isFolder && !app.trackFilter);

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
    selRows = new Set();
    anchor = null;
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
    selRows = new Set([i]);
    anchor = i;
    app.selectTrack(list[i].id);
  }

  /** Click with ⌘ toggles a row, with ⇧ selects a range, plain click selects one. */
  function clickRow(e: MouseEvent, i: number) {
    if (e.metaKey || e.ctrlKey) {
      const next = new Set(selRows);
      next.has(i) ? next.delete(i) : next.add(i);
      selRows = next;
      anchor = i;
      if (next.has(i)) app.selectTrack(list[i].id);
    } else if (e.shiftKey && anchor != null) {
      const [a, b] = anchor < i ? [anchor, i] : [i, anchor];
      selRows = new Set(Array.from({ length: b - a + 1 }, (_, k) => a + k));
      app.selectTrack(list[i].id);
    } else {
      choose(i);
    }
  }

  function scrollTo(j: number) {
    if (!rowsEl) return;
    if (j * ROW < rowsEl.scrollTop) rowsEl.scrollTop = j * ROW;
    else if ((j + 1) * ROW > rowsEl.scrollTop + viewport) rowsEl.scrollTop = (j + 1) * ROW - viewport;
  }

  function openMenu(e: MouseEvent, i: number) {
    if (!app.isLibrary) return;
    e.preventDefault();
    if (!selRows.has(i)) choose(i);
    menuFilter = "";
    // Keep the menu on screen.
    menu = { x: Math.min(e.clientX, window.innerWidth - 260), y: Math.min(e.clientY, window.innerHeight - 220), sub: null };
  }

  async function addTo(id: number) {
    const ids = selectedIds;
    menu = null;
    await app.addToPlaylist(id, ids);
  }

  async function newSet() {
    const ids = selectedIds;
    menu = null;
    await app.newSetWith(ids);
  }

  function removeSelected() {
    menu = null;
    app.removeFromPlaylist([...selRows].map((i) => order[i]));
    selRows = new Set();
  }

  function onKey(e: KeyboardEvent) {
    if (list.length === 0) return;
    const i = anchor ?? list.findIndex((t) => t.id === app.selectedTrack);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const j = Math.min(list.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)));
      if (e.shiftKey) {
        selRows = new Set([...selRows, j]);
        anchor = j;
        app.selectTrack(list[j].id);
      } else {
        choose(j);
      }
      scrollTo(j);
    } else if (e.key === "a" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      selRows = new Set(list.map((_, k) => k));
    } else if (e.key === "Escape") {
      selRows = new Set(anchor != null ? [anchor] : []);
      menu = null;
    } else if (e.key === "Enter" && app.selectedTrack != null) {
      app.detailOpen = true;
    } else if ((e.key === "Backspace" || e.key === "Delete") && editable && selRows.size > 0) {
      e.preventDefault();
      removeSelected();
    }
  }

  function dragStart(e: DragEvent, i: number) {
    if (!app.isLibrary || !e.dataTransfer) return;
    // Dragging a selected row takes the whole selection along.
    const many = selRows.has(i) && selRows.size > 1;
    e.dataTransfer.effectAllowed = "copyMove";
    e.dataTransfer.setData("application/x-rbx-tracks", JSON.stringify(many ? selectedIds : [list[i].id]));
    if (reorderable && !many) e.dataTransfer.setData("application/x-rbx-row", String(i));
  }

  function dragOver(e: DragEvent, i: number) {
    if (!reorderable || !e.dataTransfer?.types.includes("application/x-rbx-tracks")) return;
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
    {#if selRows.size > 1}
      <span class="picked"><span class="readout big">{selRows.size}</span> vybráno</span>
    {/if}
    {#if app.isLibrary && toAnalyze > 0}
      <button
        class="btn small"
        disabled={app.analyzeJob != null}
        onclick={() => app.analyzeTracks(app.currentTrackIds)}
        title="Waveform, beatgrid (Beat This!) a tónina pro skladby bez analýzy"
      >
        <AudioWaveform size={13} />
        {#if app.analyzeJob != null && job}Analyzuju {job.filesDone}/{job.filesTotal}{:else}Analyzovat ({toAnalyze}){/if}
      </button>
    {/if}
    {#if app.isLibrary && app.currentPlaylist && !app.currentPlaylist.isFolder}
      <button class="btn small plan" onclick={() => (app.view = "planner")} disabled={list.length < 2} title="Seřadí skladby podle tóniny, tempa a energie a naplánuje přechody">
        <Sparkles size={13} /> Naplánovat set
      </button>
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
    {#if canKeep}
      <button class="btn small" onclick={keepOrder} title="Uloží toto seřazení jako pořadí playlistu (půjde tak i na flashku)">
        Uložit toto pořadí
      </button>
    {/if}
    <label class="search">
      <Search size={14} />
      <input type="search" placeholder={app.selection?.kind === "all" ? "Hledat v knihovně" : "Hledat v playlistu"} bind:value={app.trackFilter} />
    </label>
  </div>

  {#snippet head(key: SortKey, label: string, cls: string)}
    <button
      class="sort {cls}"
      class:on={sort?.key === key}
      onclick={() => sortBy(key)}
      title={sort?.key === key ? (sort.desc ? "Zpět na pořadí playlistu" : "Seřadit sestupně") : `Seřadit podle: ${label}`}
    >
      <span>{label}</span>
      {#if sort?.key === key}
        {#if sort.desc}<ArrowDown size={11} />{:else}<ArrowUp size={11} />{/if}
      {/if}
    </button>
  {/snippet}

  <div class="cols">
    <button
      class="sort c-no"
      class:on={!sort}
      onclick={() => (sort = null)}
      title={app.selection?.kind === "playlist" ? "Pořadí playlistu" : "Výchozí pořadí"}
    >
      {#if sort}<ArrowUpDown size={11} />{:else}#{/if}
    </button>
    <span class="c-st"></span>
    {@render head("title", "Název", "c-title")}
    {@render head("artist", "Interpret", "c-artist")}
    {@render head("bpm", "BPM", "c-bpm r")}
    {@render head("key", "Tónina", "c-key")}
    {@render head("time", "Čas", "c-time r")}
    {@render head("format", "Formát", "c-fmt")}
    {@render head("added", "Přidáno", "c-date")}
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
          class:selected={selRows.has(i) || (selRows.size === 0 && t.id === app.selectedTrack)}
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
          onclick={(e) => clickRow(e, i)}
          oncontextmenu={(e) => openMenu(e, i)}
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
          <span class="c-date readout num">{date(t.dateAdded)}</span>
          <span class="c-cue">
            {#each Array(8) as _, s}
              {@const c = app.cues[t.id]?.[s]}
              <i class:off={!c} style:background={c ? rgb(c, s + 1) : undefined}></i>
            {/each}
            {#if app.memCount[t.id]}
              <b class="mem" title="{app.memCount[t.id]} memory cue">▾{app.memCount[t.id]}</b>
            {/if}
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

<svelte:window onclick={(e) => menu && !(e.target as HTMLElement).closest(".ctx") && (menu = null)} />

{#if menu}
  {@const n = selectedIds.length}
  {@const q = menuFilter.trim().toLowerCase()}
  <div class="ctx" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <div class="ctx-head">{n} {plural(n, "skladba", "skladby", "skladeb")}</div>
    <button role="menuitem" class:open={menu.sub === "sets"} onmouseenter={() => menu && (menu.sub = "sets")} onclick={() => menu && (menu.sub = "sets")}>
      <Sparkles size={13} /> Vložit do setu <ChevronRight size={13} class="chev" />
    </button>
    <button role="menuitem" class:open={menu.sub === "playlists"} onmouseenter={() => menu && (menu.sub = "playlists")} onclick={() => menu && (menu.sub = "playlists")}>
      <ListMusic size={13} /> Vložit do playlistu <ChevronRight size={13} class="chev" />
    </button>
    <button role="menuitem" onmouseenter={() => menu && (menu.sub = null)} onclick={newSet}>
      <ListPlus size={13} /> Nový set z výběru
    </button>
    {#if app.unanalyzed(selectedIds).length > 0}
      {@const k = app.unanalyzed(selectedIds).length}
      <button role="menuitem" onmouseenter={() => menu && (menu.sub = null)} onclick={() => ((menu = null), app.analyzeTracks(selectedIds))}>
        <AudioWaveform size={13} /> Analyzovat ({k}) <span class="hint">waveform, grid, tónina</span>
      </button>
    {/if}
    {#if editable}
      <div class="sep"></div>
      <button role="menuitem" class="danger" onmouseenter={() => menu && (menu.sub = null)} onclick={removeSelected}>
        <Trash size={13} /> Odebrat z „{app.currentTitle}“
      </button>
    {/if}

    {#if menu.sub}
      {@const items = (menu.sub === "sets" ? sets : plainLists).filter((p) => !q || p.name.toLowerCase().includes(q))}
      <div class="sub" class:left={menu.x > window.innerWidth - 540}>
        {#if (menu.sub === "sets" ? sets : plainLists).length > 8}
          <!-- svelte-ignore a11y_autofocus -->
          <input type="search" placeholder="Hledat" bind:value={menuFilter} autofocus />
        {/if}
        <div class="sub-list">
          {#each items as p (p.id)}
            {@const has = selectedIds.filter((id) => p.trackIds.includes(id)).length}
            <button role="menuitem" onclick={() => addTo(p.id)} disabled={has === n}>
              <span class="pn">{p.name}</span>
              <span class="readout cnt">{has === n ? "už tam jsou" : p.trackIds.length}</span>
            </button>
          {/each}
          {#if items.length === 0}
            <p class="none">{menu.sub === "sets" ? "Zatím nemáš žádný set." : "Nic nenalezeno."}</p>
          {/if}
        </div>
        {#if menu.sub === "sets"}
          <div class="sep"></div>
          <button role="menuitem" onclick={newSet}><ListPlus size={13} /> Nový set z výběru</button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

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
  .btn.plan {
    margin-left: auto;
    align-self: center;
    border-color: var(--cue);
    color: var(--cue);
  }
  .btn.plan + .smart-wrap {
    margin-left: 0;
  }
  .picked {
    color: var(--cue);
  }
  .picked .big {
    color: var(--cue);
  }
  .ctx {
    position: fixed;
    width: 240px;
    padding: 6px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
    z-index: 30;
  }
  .ctx-head {
    padding: 4px 10px 6px;
    font-size: 11px;
    color: var(--text-2);
  }
  .ctx .hint {
    margin-left: auto;
    padding-left: 10px;
    font-size: 11px;
    color: var(--text-3);
  }
  .ctx button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 13px;
    color: var(--text);
  }
  .ctx button:hover:not(:disabled),
  .ctx button.open {
    background: var(--panel-2);
  }
  .ctx button:disabled {
    color: var(--text-3);
  }
  .ctx :global(.chev) {
    margin-left: auto;
    color: var(--text-2);
  }
  .ctx .danger {
    color: #f2a3a5;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }
  .sub {
    position: absolute;
    top: 0;
    left: calc(100% + 4px);
    width: 260px;
    padding: 6px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
  }
  .sub.left {
    left: auto;
    right: calc(100% + 4px);
  }
  .sub input {
    width: 100%;
    margin-bottom: 4px;
  }
  .sub-list {
    max-height: 300px;
    overflow-y: auto;
  }
  .pn {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cnt {
    font-size: 12px;
    color: var(--text-2);
  }
  .none {
    margin: 6px 10px;
    font-size: 12px;
    color: var(--text-3);
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
    grid-template-columns: 24px 16px minmax(160px, 1fr) minmax(120px, 0.62fr) 52px 56px 40px 84px 72px 110px;
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
  .sort {
    display: flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .sort span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sort.r {
    justify-content: flex-end;
  }
  .sort:hover {
    color: var(--text);
  }
  .sort.on {
    color: var(--cue);
  }
  .sort.c-no.on {
    color: inherit;
  }
  .c-date {
    font-size: 13px;
    color: var(--text-2);
    white-space: nowrap;
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
  .c-cue .mem {
    margin-left: 4px;
    font-size: 10px;
    font-weight: 600;
    line-height: 7px;
    color: var(--fault);
  }
  .c-cue i.off {
    background: var(--line);
  }
  .empty {
    padding: 24px 16px;
    color: var(--text-2);
  }
</style>
