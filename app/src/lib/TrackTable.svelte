<script lang="ts">
  import {
    ArrowDown,
    ArrowUp,
    ArrowUpDown,
    AudioLines,
    AudioWaveform,
    ChevronRight,
    CircleAlert,
    ListMusic,
    ListPlus,
    Music,
    Search,
    Sparkles,
    Trash,
  } from "@lucide/svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { untrack } from "svelte";
  import type { Track } from "./api";
  import { collator, date, keyColor, keyOrder, num, rgb, time } from "./format";
  import { t } from "./i18n.svelte";
  import SetFlow from "./SetFlow.svelte";
  import { prefs } from "./prefs.svelte";
  import { app } from "./state.svelte";

  const ROW = 46;
  /** Quick filter chip over the list. */
  type Chip = "all" | "nocue" | "noanalysis" | "bad";
  let chip = $state<Chip>("all");
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

  /** Cue summary known and no hot cue at all. */
  const noCue = (id: number) => id in app.cues && !app.cues[id].some((c) => c);
  const chipTest: Record<Chip, (t: Track) => boolean> = {
    all: () => true,
    nocue: (t) => noCue(t.id),
    noanalysis: (t) => !t.missing && t.analyzed === false,
    bad: (t) => app.errorTracks.has(t.id),
  };
  const counts = $derived.by(() => {
    const ts = app.currentTracks;
    const cuesKnown = ts.every((t) => t.id in app.cues);
    return {
      all: ts.length,
      nocue: cuesKnown ? ts.filter((t) => noCue(t.id)).length : null,
      noanalysis: ts.filter(chipTest.noanalysis).length,
      bad: ts.filter(chipTest.bad).length,
    };
  });

  /** Row → position in the unsorted list (a track can be in a playlist twice). */
  const order = $derived.by(() => {
    const tracks = app.currentTracks;
    const test = chipTest[chip];
    const idx = tracks.map((_, i) => i).filter((i) => chip === "all" || test(tracks[i]));
    if (!sort) return idx;
    const { key, desc } = sort;
    const vals = tracks.map(value[key]);
    const coll = collator();
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
    app.notice = t("library.table.orderSaved", { name: p.name });
  }
  const aiTodo = $derived(app.currentTrackIds.filter((id) => !app.aiAnalyzed.has(id)).length);
  const total = $derived(list.reduce((s, t) => s + t.duration, 0));
  const toAnalyze = $derived(app.isLibrary ? app.unanalyzed(app.currentTrackIds).length : 0);
  const job = $derived(app.jobs.find((j) => j.id === app.analyzeJob));
  const bad = $derived(list.filter((t) => app.errorTracks.has(t.id)).length);
  // Rows map to playlist positions only unfiltered; dragging rows into place only in its own order.
  const editable = $derived(app.isLibrary && app.selection?.kind === "playlist" && !app.trackFilter);
  const reorderable = $derived(editable && !sort && chip === "all");
  const canKeep = $derived(
    !!sort && chip === "all" && app.isLibrary && !!app.currentPlaylist && !app.currentPlaylist.isFolder && !app.trackFilter,
  );
  const isSet = $derived(app.isLibrary && !!app.currentPlaylist?.isSet);
  /** The set in its own order, for the flow strip (not the search or sort). */
  const setTracks = $derived(
    isSet ? (app.currentPlaylist?.trackIds ?? []).map((id) => app.trackById.get(id)).filter((t): t is Track => !!t) : [],
  );
  const deckId = $derived(app.deckHere ? app.deck?.track.id : undefined);

  // Only the rows in view are rendered.
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 10));
  const last = $derived(Math.min(list.length, Math.ceil((scrollTop + viewport) / ROW) + 10));
  const visible = $derived(list.slice(first, last));

  $effect(() => {
    const ids = visible.map((t) => t.id);
    const timer = setTimeout(() => app.loadCues(ids), 60);
    return () => clearTimeout(timer);
  });

  // Short lists get their cue summary up front, so "Bez cue" can be counted.
  $effect(() => {
    const ids = app.currentTrackIds;
    if (ids.length <= 600) untrack(() => app.loadCues(ids));
  });

  function pickChip(c: Chip) {
    chip = chip === c && c !== "all" ? "all" : c;
    if (chip === "nocue") app.loadCues(app.currentTrackIds);
  }

  $effect(() => {
    void app.selection;
    chip = "all";
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
    const track = app.trackById.get(id);
    if (!app.isLibrary || !track) return "";
    if (track.missing) return t("library.table.missingFile");
    if (track.analyzed === false) return t("library.noAnalysisHint");
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
  {#if !app.isLibrary}
    <div class="subhead">
      <h2>{app.currentTitle}</h2>
      <span><b class="readout">{num(list.length)}</b> {t("library.stats.tracks", { count: list.length })}</span>
      <span><b class="readout">{time(total)}</b> {t("library.stats.total")}</span>
      {#if bad > 0}
        <span class="fault"><b class="readout">{num(bad)}</b> {t("library.stats.wontPlay", { count: bad })}</span>
      {/if}
    </div>
  {/if}

  {#if isSet && setTracks.length > 0}
    <SetFlow tracks={setTracks} />
  {/if}

  <div class="toolbar">
    <label class="search">
      <Search size={14} />
      <input
        type="search"
        placeholder={app.selection?.kind === "all" ? t("library.table.searchLibrary") : isSet ? t("library.table.searchSet") : app.isLibrary ? t("library.table.searchPlaylist") : t("library.table.searchStick")}
        bind:value={app.trackFilter}
      />
    </label>
    <div class="chips">
      <button class="chip" class:on={chip === "all"} onclick={() => pickChip("all")}>
        {t("library.table.chipAll")} <span class="readout">{num(counts.all)}</span>
      </button>
      <button class="chip" class:on={chip === "nocue"} onclick={() => pickChip("nocue")} title={t("library.table.chipNoCueHint")}>
        {t("library.table.chipNoCue")} {#if counts.nocue != null}<span class="readout">{num(counts.nocue)}</span>{/if}
      </button>
      {#if app.isLibrary}
        <button class="chip" class:on={chip === "noanalysis"} onclick={() => pickChip("noanalysis")} title={t("library.noAnalysisHint")}>
          {t("library.table.chipNoAnalysis")} <span class="readout">{num(counts.noanalysis)}</span>
        </button>
      {:else if app.verify}
        <button class="chip" class:on={chip === "bad"} onclick={() => pickChip("bad")} title={t("library.table.chipBadHint")}>
          {t("library.table.chipBad")} <span class="readout">{num(counts.bad)}</span>
        </button>
      {/if}
    </div>
    {#if selRows.size > 1}
      <span class="picked"><b class="readout">{num(selRows.size)}</b> {t("library.stats.selected")}</span>
    {/if}

    <div class="tools">
      {#if app.isLibrary && toAnalyze > 0}
        <button
          class="btn ghost"
          onclick={() => app.analyzeTracks(app.currentTrackIds)}
          title={app.analyzeJob != null
            ? t("library.table.addToAnalysis")
            : t("library.table.analyzeTip")}
        >
          <AudioWaveform size={14} />
          {#if app.analyzeJob != null && job}{t("library.table.analyzing", { done: job.filesDone, total: job.filesTotal })}{:else}{t("library.analyze", { count: toAnalyze })}{/if}
        </button>
      {/if}
      {#if canKeep}
        <button class="btn ghost" onclick={keepOrder} title={t("library.table.keepOrderHint")}>
          {t("library.table.keepOrder")}
        </button>
      {/if}
      {#if prefs.value.general.helpers}
      <div class="smart-wrap">
        <button class="btn ghost" onclick={() => (smartMenu = !smartMenu)} disabled={list.length === 0 || app.selection?.kind === "all"}>
          <Sparkles size={14} /> {t("library.table.smartCue")}
        </button>
        {#if smartMenu}
          <div class="smart-menu" role="menu">
            {#if app.aiInstalled && aiTodo > 0}
              <button role="menuitem" disabled={app.aiJob != null} onclick={() => ((smartMenu = false), app.aiAnalyze(app.currentTrackIds))}>
                <span>{app.aiJob != null ? t("library.table.aiRunning") : t("library.table.aiFirst", { count: aiTodo })}</span>
                <span class="desc">{t("library.table.aiFirstHint")}</span>
              </button>
            {/if}
            <button role="menuitem" onclick={() => ((smartMenu = false), app.smartCuePlaylist(false))}>
              <span>{t("library.table.onlyNoCue")}</span>
              <span class="desc">{t("library.table.onlyNoCueHint")}</span>
            </button>
            <button role="menuitem" onclick={() => ((smartMenu = false), app.smartCuePlaylist(true))}>
              <span>{t("library.table.overwrite")}</span>
              <span class="desc">{t("library.table.overwriteHint")}</span>
            </button>
          </div>
        {/if}
      </div>
      {/if}
      {#if prefs.value.general.helpers && app.isLibrary && app.currentPlaylist && !app.currentPlaylist.isFolder}
        <button class="btn" onclick={() => (app.view = "planner")} disabled={list.length < 2} title={t("library.table.planSetHint")}>
          <ListMusic size={14} /> {t("library.table.planSet")}
        </button>
      {/if}
    </div>
  </div>

  {#snippet head(key: SortKey, label: string, cls: string)}
    <button
      class="sort {cls}"
      class:on={sort?.key === key}
      onclick={() => sortBy(key)}
      title={sort?.key === key ? (sort.desc ? t("library.table.sortBack") : t("library.table.sortDesc")) : t("library.table.sortBy", { column: label })}
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
      title={app.selection?.kind === "playlist" ? t("library.table.playlistOrder") : t("library.table.defaultOrder")}
    >
      {#if sort}<ArrowUpDown size={11} />{:else}#{/if}
    </button>
    <span class="c-track">
      {@render head("title", t("library.table.columns.track"), "")}
      <span class="slash">/</span>
      {@render head("artist", t("library.table.columns.artist"), "")}
    </span>
    {@render head("bpm", "BPM", "c-bpm r")}
    {@render head("key", t("library.table.columns.key"), "c-key")}
    {@render head("time", t("library.table.columns.time"), "c-time r")}
    <span class="c-cue">{t("library.table.columns.cue")}</span>
    {@render head("format", t("library.table.columns.file"), "c-fmt")}
    {@render head("added", t("library.table.columns.added"), "c-date r")}
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="rows"
    class:filedrop={app.fileDrag && app.fileTarget != null && app.fileTarget === app.currentPlaylist?.id}
    data-tracks
    tabindex="0"
    role="listbox"
    aria-label={t("library.table.tracksAria")}
    bind:this={rowsEl}
    bind:clientHeight={viewport}
    onscroll={(e) => (scrollTop = (e.currentTarget as HTMLElement).scrollTop)}
    onkeydown={onKey}
    ondrop={dropRow}
    ondragleave={(e) => !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node) && (dropAt = null)}
  >
    <div style:height="{list.length * ROW}px" class="space">
      {#each visible as tr, k (`${first + k}:${tr.id}`)}
        {@const i = first + k}
        {@const st = statusClass(tr.id)}
        {@const fmt = app.formatOverride.get(tr.id) ?? tr.format}
        {@const sev = app.status.get(tr.id)}
        {@const hot = app.cues[tr.id]}
        <div
          class="row"
          class:selected={selRows.has(i) || (selRows.size === 0 && tr.id === app.selectedTrack)}
          class:deck={tr.id === deckId}
          class:drop-before={dropAt === i}
          class:drop-after={dropAt === i + 1 && i === list.length - 1}
          class:missing={tr.missing}
          style:top="{i * ROW}px"
          role="option"
          aria-selected={tr.id === app.selectedTrack}
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
          <span class="c-no readout">
            {#if tr.id === deckId}<AudioLines size={14} />{:else}{i + 1}{/if}
          </span>
          <span class="c-track">
            <span class="thumb" style:--k={keyColor(tr.key)}>
              <Music size={13} />
              {#if tr.artwork}
                <img src={convertFileSrc(tr.artwork)} alt="" loading="lazy" onerror={(e) => ((e.currentTarget as HTMLElement).style.display = "none")} />
              {/if}
              {#if st === "fault" || st === "warn"}<i class="st dot {st}" title={statusTitle(tr.id)}></i>{/if}
            </span>
            <span class="names">
              <span class="t">{tr.title}{#if app.pending.has(tr.id)}<i class="edited" title={t("library.table.cueEdited")}></i>{/if}</span>
              <span class="a">{tr.artist}</span>
            </span>
          </span>
          <span class="c-bpm r readout">{tr.bpm ? tr.bpm.toFixed(2) : "–"}</span>
          <span class="c-key">{#if tr.key}<span class="keypill" style:--k={keyColor(tr.key)}>{tr.key}</span>{/if}</span>
          <span class="c-time r readout">{time(tr.duration)}</span>
          <span class="c-cue">
            {#if hot && !hot.some((c) => c)}
              <span class="nocue"><CircleAlert size={13} /> {t("library.table.noCue")}</span>
            {:else}
              {#each Array(8) as _, s}
                {@const c = hot?.[s]}
                <i class:off={!c} style:background={c ? rgb(c, s + 1) : undefined}></i>
              {/each}
            {/if}
            {#if app.memCount[tr.id]}
              <b class="mem" title={t("library.table.memoryCues", { count: app.memCount[tr.id] })}>▾{app.memCount[tr.id]}</b>
            {/if}
          </span>
          <span class="c-fmt" class:fault={sev === "error"} class:warn={sev === "warning"}>{fmt}</span>
          <span class="c-date r readout">{date(tr.dateAdded)}</span>
        </div>
      {/each}
    </div>
    {#if list.length === 0}
      <p class="empty">
        {#if chip !== "all" && app.currentTracks.length > 0}{t("library.table.emptyFilter")}
        {:else if app.selection?.kind === "player"}{t("library.table.emptyPlayer")}
        {:else if app.isLibrary && app.selection?.kind === "playlist"}{t("library.table.emptyPlaylist")}
        {:else if app.isLibrary && (app.data?.tracks.length ?? 0) === 0}{t("library.table.emptyLibrary")}
        {:else}{t("library.table.empty")}{/if}
      </p>
    {/if}
  </div>
</section>

<svelte:window onclick={(e) => menu && !(e.target as HTMLElement).closest(".ctx") && (menu = null)} />

{#if menu}
  {@const n = selectedIds.length}
  {@const q = menuFilter.trim().toLowerCase()}
  <div class="ctx" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <div class="ctx-head">{t("common.tracks", { count: n })}</div>
    <button role="menuitem" class:open={menu.sub === "sets"} onmouseenter={() => menu && (menu.sub = "sets")} onclick={() => menu && (menu.sub = "sets")}>
      <Sparkles size={13} /> {t("library.table.addToSet")} <ChevronRight size={13} class="chev" />
    </button>
    <button role="menuitem" class:open={menu.sub === "playlists"} onmouseenter={() => menu && (menu.sub = "playlists")} onclick={() => menu && (menu.sub = "playlists")}>
      <ListMusic size={13} /> {t("library.table.addToPlaylist")} <ChevronRight size={13} class="chev" />
    </button>
    <button role="menuitem" onmouseenter={() => menu && (menu.sub = null)} onclick={newSet}>
      <ListPlus size={13} /> {t("library.table.newSetFromSelection")}
    </button>
    {#if app.unanalyzed(selectedIds).length > 0}
      {@const k = app.unanalyzed(selectedIds).length}
      <button role="menuitem" onmouseenter={() => menu && (menu.sub = null)} onclick={() => ((menu = null), app.analyzeTracks(selectedIds))}>
        <AudioWaveform size={13} /> {t("library.analyze", { count: k })} <span class="hint">{t("library.analyzeHint")}</span>
      </button>
    {/if}
    {#if editable}
      <div class="sep"></div>
      <button role="menuitem" class="danger" onmouseenter={() => menu && (menu.sub = null)} onclick={removeSelected}>
        <Trash size={13} /> {t("library.table.removeFrom", { name: app.currentTitle })}
      </button>
    {/if}

    {#if menu.sub}
      {@const items = (menu.sub === "sets" ? sets : plainLists).filter((p) => !q || p.name.toLowerCase().includes(q))}
      <div class="sub" class:left={menu.x > window.innerWidth - 540}>
        {#if (menu.sub === "sets" ? sets : plainLists).length > 8}
          <!-- svelte-ignore a11y_autofocus -->
          <input type="search" placeholder={t("library.table.search")} bind:value={menuFilter} autofocus />
        {/if}
        <div class="sub-list">
          {#each items as p (p.id)}
            {@const has = selectedIds.filter((id) => p.trackIds.includes(id)).length}
            <button role="menuitem" onclick={() => addTo(p.id)} disabled={has === n}>
              <span class="pn">{p.name}</span>
              <span class="readout cnt">{has === n ? t("library.table.alreadyThere") : p.trackIds.length}</span>
            </button>
          {/each}
          {#if items.length === 0}
            <p class="none">{menu.sub === "sets" ? t("library.table.noSets") : t("library.table.notFound")}</p>
          {/if}
        </div>
        {#if menu.sub === "sets"}
          <div class="sep"></div>
          <button role="menuitem" onclick={newSet}><ListPlus size={13} /> {t("library.table.newSetFromSelection")}</button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .table {
    container-type: inline-size;
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .subhead {
    display: flex;
    align-items: baseline;
    gap: 16px;
    padding: 2px 28px 14px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }
  .subhead b {
    font-size: 16px;
    color: var(--text);
  }
  .subhead .fault b {
    color: var(--danger);
  }

  /* Toolbar: search, filter chips, playlist actions */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 28px 12px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 300px;
    flex: 0 1 300px;
    min-width: 160px;
    height: 32px;
    padding: 0 12px;
    border-radius: var(--r-control);
    background: var(--surface-2);
    color: var(--text-3);
    border: 1px solid transparent;
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0;
    border: 0;
    background: transparent;
    font-size: 13px;
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  .chips {
    display: flex;
    gap: 6px;
    margin-left: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border-radius: 14px;
    border: 1px solid var(--border);
    background: none;
    color: var(--text-2);
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
  }
  .chip .readout {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-3);
  }
  .chip:hover {
    color: var(--text);
  }
  .chip.on {
    border-color: transparent;
    background: var(--surface-3);
    color: var(--text);
    font-weight: 600;
  }
  .picked {
    margin-left: 8px;
    font-size: 12.5px;
    color: var(--accent);
    white-space: nowrap;
  }
  .picked b {
    font-size: 15px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .smart-wrap {
    position: relative;
  }
  .smart-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 6px);
    width: 290px;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--r-panel);
    box-shadow: var(--shadow-pop);
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
    border-radius: var(--r-control);
    background: none;
    text-align: left;
    color: var(--text);
    font-size: 13px;
  }
  .smart-menu button:hover {
    background: var(--surface-3);
  }
  .smart-menu .desc {
    font-size: 11.5px;
    color: var(--text-3);
  }

  /* Table */
  .cols,
  .row {
    display: grid;
    grid-template-columns: 30px minmax(180px, 1fr) 64px 62px 58px 128px 76px 82px;
    gap: 12px;
    align-items: center;
    padding: 0 12px 0 8px;
  }
  .cols {
    height: 34px;
    margin: 0 28px;
    border-bottom: 1px solid var(--hairline);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .cols .c-track {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 44px;
  }
  .cols .c-cue {
    padding-left: 0;
  }
  .slash {
    opacity: 0.5;
  }
  .sort {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
    text-transform: inherit;
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
    color: var(--text-2);
  }
  .sort.on {
    color: var(--text);
  }
  .sort.c-no {
    justify-content: center;
  }
  .sort.c-no.on {
    color: inherit;
  }
  .rows {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 4px 28px 12px;
    outline: none;
  }
  .space {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: 46px;
    border-radius: 10px;
    font-size: 13px;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row.selected {
    background: var(--surface-3);
  }
  .row.deck {
    background: var(--accent-soft);
  }
  .row.deck .c-no {
    color: var(--accent);
  }
  .row.missing .names {
    opacity: 0.45;
  }
  .rows.filedrop {
    outline: 2px dashed rgb(242 163 58 / 0.6);
    outline-offset: -6px;
    background: rgb(242 163 58 / 0.04);
  }
  .row.drop-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .row.drop-after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .r {
    text-align: right;
  }
  .c-no {
    display: flex;
    justify-content: center;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-3);
  }
  .c-track {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .thumb {
    position: relative;
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--k) 22%, var(--surface-3));
    color: color-mix(in srgb, var(--k) 40%, var(--text-2));
  }
  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border-radius: 7px;
    object-fit: cover;
  }
  .thumb .st {
    position: absolute;
    right: -2px;
    top: -2px;
    width: 9px;
    height: 9px;
    box-shadow: 0 0 0 2px var(--surface-1);
  }
  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
  }
  .names .t,
  .names .a {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .names .t {
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text);
  }
  .names .a {
    font-size: 12px;
    color: var(--text-3);
  }
  .edited {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 7px;
    border-radius: 50%;
    background: var(--accent);
    vertical-align: middle;
  }
  .row .c-bpm,
  .row .c-time {
    font-size: 15px;
  }
  .c-key {
    display: flex;
    justify-content: center;
  }
  .cols .c-key {
    justify-content: center;
  }
  .c-cue {
    display: flex;
    align-items: center;
    gap: 3px;
    padding-left: 8px;
  }
  .c-cue i {
    width: 8px;
    height: 14px;
    border-radius: 2px;
  }
  .c-cue i.off {
    background: var(--surface-3);
  }
  .c-cue .mem {
    margin-left: 6px;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--danger);
  }
  .nocue {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--warning);
  }
  .c-fmt {
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .c-fmt.fault {
    color: var(--danger);
  }
  .c-fmt.warn {
    color: var(--warning);
  }
  .row .c-date {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-3);
    white-space: nowrap;
  }
  /* Narrow (detail panel open): file and date columns go. */
  @container (max-width: 980px) {
    .cols,
    .row {
      grid-template-columns: 30px minmax(160px, 1fr) 64px 62px 58px 128px;
    }
    .c-fmt,
    .c-date {
      display: none;
    }
  }
  .empty {
    padding: 24px 12px;
    color: var(--text-3);
  }

  /* Context menu */
  .ctx {
    position: fixed;
    width: 240px;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--r-panel);
    box-shadow: var(--shadow-pop);
    z-index: 30;
  }
  .ctx-head {
    padding: 4px 10px 6px;
    font-size: 11.5px;
    color: var(--text-3);
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
    padding: 7px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    font-size: 13px;
    color: var(--text);
  }
  .ctx button :global(svg) {
    color: var(--text-2);
  }
  .ctx button:hover:not(:disabled),
  .ctx button.open {
    background: var(--surface-3);
  }
  .ctx button:disabled {
    color: var(--text-3);
  }
  .ctx :global(.chev) {
    margin-left: auto;
  }
  .ctx .danger,
  .ctx .danger :global(svg) {
    color: var(--danger);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--hairline);
  }
  .sub {
    position: absolute;
    top: 0;
    left: calc(100% + 4px);
    width: 260px;
    padding: 6px;
    background: var(--surface-2);
    border-radius: var(--r-panel);
    box-shadow: var(--shadow-pop);
  }
  .sub.left {
    left: auto;
    right: calc(100% + 4px);
  }
  .sub input {
    width: 100%;
    margin-bottom: 4px;
    background: var(--surface-3);
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
    font-size: 12.5px;
    color: var(--text-3);
  }
  .none {
    margin: 6px 10px;
    font-size: 12px;
    color: var(--text-3);
  }
</style>
