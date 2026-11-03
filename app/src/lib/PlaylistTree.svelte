<script lang="ts">
  import { ArrowDown, ArrowDownUp, ArrowUp, Check, ChevronDown, ChevronRight, Clock, FolderPlus, Library, ListMusic, ListPlus, Search, Sparkles } from "@lucide/svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import { libApi, type Playlist } from "./api";
  import { app } from "./state.svelte";

  let collapsed = $state<Record<number, boolean>>({});
  let menu = $state<{ node: Playlist; x: number; y: number } | null>(null);
  let dropTarget = $state<number | null>(null);
  /** Playlist being dragged in the tree, and where it would land. */
  let dragging = $state<number | null>(null);
  let moveTarget = $state<{ id: number; after: boolean } | null>(null);
  let renameValue = $state("");
  let nav = $state<HTMLElement>();
  /** Files dragged in from Finder: over the window, and the row under them. */
  let fileDrag = $state(false);
  let fileTarget = $state<number | null>(null);

  /** The playlist row at a window position (physical pixels from Tauri). */
  function rowAt(pos: { x: number; y: number }): number | null {
    const dpr = window.devicePixelRatio || 1;
    const el = document.elementFromPoint(pos.x / dpr, pos.y / dpr) as HTMLElement | null;
    // Rows of a stick are not library playlists: then it all goes to the top.
    if (!el || !nav?.contains(el) || !app.isLibrary) return null;
    const id = el.closest<HTMLElement>("[data-pid]")?.dataset.pid;
    return id ? Number(id) : null;
  }

  onMount(() => {
    const off = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      // Drags inside the app carry no files and are left to the page.
      if (p.type === "enter") {
        fileDrag = p.paths.length > 0;
        if (fileDrag) fileTarget = rowAt(p.position);
      } else if (p.type === "over") {
        if (fileDrag) fileTarget = rowAt(p.position);
      } else if (p.type === "drop") {
        const target = rowAt(p.position);
        fileDrag = false;
        fileTarget = null;
        if (p.paths.length) app.addFolders(p.paths, target);
      } else {
        fileDrag = false;
        fileTarget = null;
      }
    });
    return () => {
      off.then((f) => f());
    };
  });

  const q = $derived(app.playlistFilter.trim().toLowerCase());
  const lib = $derived(app.isLibrary);

  // ---- sorting the list -------------------------------------------------

  type Order = "manual" | "name" | "name-desc" | "date-desc" | "date" | "tracks-desc";
  const ORDERS: [Order, string][] = [
    ["manual", "Vlastní pořadí"],
    ["name", "Název A → Z"],
    ["name-desc", "Název Z → A"],
    ["date-desc", "Datum v názvu: nejnovější"],
    ["date", "Datum v názvu: nejstarší"],
    ["tracks-desc", "Počet skladeb"],
  ];
  /** Separate for sets and playlists; kept across restarts. */
  let order = $state<Record<"sets" | "lists", Order>>({
    sets: (localStorage.getItem("order-sets") as Order) || "manual",
    lists: (localStorage.getItem("order-lists") as Order) || "manual",
  });
  $effect(() => {
    localStorage.setItem("order-sets", order.sets);
    localStorage.setItem("order-lists", order.lists);
  });
  let orderMenu = $state<{ which: "sets" | "lists"; x: number; y: number } | null>(null);

  const coll = new Intl.Collator("cs", { numeric: true, sensitivity: "base" });

  /** A date in the name ("17.4.2026", "22.3.26 - MID", "2026-04-17") as yyyymmdd. */
  function nameDate(name: string): number | null {
    let m = /(\d{4})-(\d{1,2})-(\d{1,2})/.exec(name);
    if (m) return +m[1] * 10000 + +m[2] * 100 + +m[3];
    m = /(?:^|\D)(\d{1,2})[./](\d{1,2})[./](\d{4}|\d{2})(?!\d)/.exec(name);
    if (!m) return null;
    const y = m[3].length === 2 ? 2000 + +m[3] : +m[3];
    const [d, mo] = [+m[1], +m[2]];
    return mo >= 1 && mo <= 12 && d >= 1 && d <= 31 ? y * 10000 + mo * 100 + d : null;
  }

  function arrange(list: Playlist[], o: Order): Playlist[] {
    if (o === "manual") return list;
    const byName = (a: Playlist, b: Playlist) => coll.compare(a.name, b.name);
    const sorted = [...list].sort((a, b) => {
      // Folders stay on top, as in Finder.
      if (a.isFolder !== b.isFolder) return a.isFolder ? -1 : 1;
      if (o === "name") return byName(a, b);
      if (o === "name-desc") return -byName(a, b);
      if (o === "tracks-desc") return b.trackIds.length - a.trackIds.length || byName(a, b);
      // Names without a date go last, by name.
      const [da, db] = [nameDate(a.name), nameDate(b.name)];
      if (da === null || db === null) return da === db ? byName(a, b) : da === null ? 1 : -1;
      return (o === "date" ? da - db : db - da) || byName(a, b);
    });
    return sorted;
  }

  /** Make what is shown the playlists' own order (in the library). */
  async function keepOrder(which: "sets" | "lists") {
    orderMenu = null;
    const groups: number[][] = [];
    if (which === "sets") groups.push(sets.map((p) => p.id));
    else {
      const walk = (parent: number) => {
        const kids = arrange(app.sortedChildren(parent).filter((p) => !p.isSet), order.lists);
        if (kids.length) groups.push(kids.map((p) => p.id));
        for (const k of kids) if (k.isFolder) walk(k.id);
      };
      walk(0);
    }
    try {
      await libApi.orderPlaylists(groups);
      await app.reloadLibrary();
      order[which] = "manual";
      app.notice = which === "sets" ? "Pořadí setů uloženo." : "Pořadí playlistů uloženo, takhle půjdou i na flashku.";
    } catch (e) {
      app.error = String(e);
    }
  }

  function openOrderMenu(e: MouseEvent, which: "sets" | "lists") {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    orderMenu = orderMenu?.which === which ? null : { which, x: r.left, y: r.bottom + 4 };
  }

  const sets = $derived(
    arrange(
      (app.data?.playlists ?? []).filter((p) => p.isSet).sort((a, b) => a.sortOrder - b.sortOrder),
      order.sets,
    ),
  );
  /** Moving by hand only makes sense in the own order. */
  const manual = (node: Playlist) => (node.isSet ? order.sets : order.lists) === "manual";
  const count = $derived(app.data?.playlists.filter((p) => !p.isFolder && !p.isSet).length ?? 0);

  interface Row {
    node: Playlist;
    depth: number;
  }

  /** Flattened, filtered tree in display order (sets are listed apart). */
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const seen = new Set<number>();
    const walk = (parent: number, depth: number) => {
      for (const n of arrange(app.sortedChildren(parent), order.lists)) {
        if (seen.has(n.id) || n.isSet) continue;
        seen.add(n.id);
        if (q) {
          if (!n.isFolder && n.name.toLowerCase().includes(q)) out.push({ node: n, depth: 0 });
          if (n.isFolder) walk(n.id, 0);
          continue;
        }
        out.push({ node: n, depth });
        if (n.isFolder && !collapsed[n.id]) walk(n.id, depth + 1);
      }
    };
    walk(0, 0);
    return out;
  });

  function selected(kind: "playlist" | "history", id: number) {
    const s = app.selection;
    return s?.kind === kind && s.id === id && app.view === "tracks";
  }

  $effect(() => {
    const id = app.renaming;
    if (id != null) renameValue = app.data?.playlists.find((p) => p.id === id)?.name ?? "";
  });

  function openMenu(e: MouseEvent, node: Playlist) {
    if (!lib) return;
    e.preventDefault();
    menu = { node, x: e.clientX, y: e.clientY };
  }

  // Playlists dragged within the tree change their order.
  function moveStart(e: DragEvent, node: Playlist) {
    if (!lib || q || !e.dataTransfer) return;
    dragging = node.id;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("application/x-rbx-playlist", String(node.id));
  }

  function moveOver(e: DragEvent, node: Playlist) {
    if (dragging == null) return false;
    const from = app.data?.playlists.find((p) => p.id === dragging);
    // Sets stay among sets, playlists and folders among themselves.
    if (!from || from.isSet !== node.isSet || node.id === dragging) return true;
    e.preventDefault();
    e.dataTransfer!.dropEffect = "move";
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    moveTarget = { id: node.id, after: e.clientY > r.top + r.height / 2 };
    return true;
  }

  function moveEnd() {
    dragging = null;
    moveTarget = null;
  }

  function moveDrop(e: DragEvent) {
    const id = dragging;
    const t = moveTarget;
    moveEnd();
    if (id == null || !t) return false;
    e.preventDefault();
    app.placePlaylist(id, t.id, t.after);
    return true;
  }

  // ⌥↑ / ⌥↓ move the selected playlist.
  function onKey(e: KeyboardEvent) {
    if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    if ((e.target as HTMLElement)?.tagName === "INPUT" || app.selection?.kind !== "playlist") return;
    const node = app.data?.playlists.find((p) => p.id === (app.selection as { id: number }).id);
    if (!node || !manual(node)) return;
    e.preventDefault();
    app.shiftPlaylist(app.selection.id, e.key === "ArrowUp" ? -1 : 1);
  }

  // Tracks dragged from the table land in a playlist.
  function dragOver(e: DragEvent, node: Playlist) {
    if (moveOver(e, node)) return;
    if (!lib || node.isFolder || !e.dataTransfer?.types.includes("application/x-rbx-tracks")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    dropTarget = node.id;
  }

  function drop(e: DragEvent, node: Playlist) {
    dropTarget = null;
    if (moveDrop(e)) return;
    const raw = e.dataTransfer?.getData("application/x-rbx-tracks");
    if (!raw || node.isFolder) return;
    e.preventDefault();
    app.addToPlaylist(node.id, JSON.parse(raw) as number[]);
  }

  function autofocus(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<svelte:window onclick={() => ((menu = null), (orderMenu = null))} onkeydown={onKey} />

{#snippet item(node: Playlist, depth: number)}
  {#if app.renaming === node.id}
    <div class="item" style:padding-left="{8 + depth * 18}px">
      <input
        class="rename"
        use:autofocus
        bind:value={renameValue}
        onkeydown={(e) => {
          if (e.key === "Enter") app.renamePlaylist(node.id, renameValue);
          if (e.key === "Escape") app.renaming = null;
        }}
        onblur={() => app.renamePlaylist(node.id, renameValue)}
      />
    </div>
  {:else if node.isFolder}
    <button
      data-pid={node.id}
      class="item folder"
      class:drop={fileTarget === node.id}
      style:padding-left="{8 + depth * 18}px"
      class:before={moveTarget?.id === node.id && !moveTarget.after}
      class:after={moveTarget?.id === node.id && moveTarget.after}
      class:lifted={dragging === node.id}
      draggable={lib && !q && manual(node)}
      onclick={() => (collapsed[node.id] = !collapsed[node.id])}
      oncontextmenu={(e) => openMenu(e, node)}
      ondragstart={(e) => moveStart(e, node)}
      ondragend={moveEnd}
      ondragover={(e) => moveOver(e, node)}
      ondrop={moveDrop}
    >
      {#if collapsed[node.id]}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
      <span class="name">{node.name}</span>
      {@render shift(node)}
    </button>
  {:else}
    <button
      class="item"
      class:selected={selected("playlist", node.id)}
      data-pid={node.id}
      class:drop={dropTarget === node.id || fileTarget === node.id}
      class:before={moveTarget?.id === node.id && !moveTarget.after}
      class:after={moveTarget?.id === node.id && moveTarget.after}
      class:lifted={dragging === node.id}
      draggable={lib && !q && manual(node)}
      ondragstart={(e) => moveStart(e, node)}
      ondragend={moveEnd}
      style:padding-left="{8 + depth * 18 + (depth > 0 ? 4 : 0)}px"
      onclick={() => app.select({ kind: "playlist", id: node.id })}
      ondblclick={() => lib && (app.renaming = node.id)}
      oncontextmenu={(e) => openMenu(e, node)}
      ondragover={(e) => dragOver(e, node)}
      ondragleave={() => (dropTarget = null)}
      ondrop={(e) => drop(e, node)}
    >
      {#if node.isSet}<Sparkles size={14} class="icon set" />{:else}<ListMusic size={14} class="icon" />{/if}
      <span class="name">{node.name}</span>
      <span class="readout n">{node.trackIds.length}</span>
      {@render shift(node)}
    </button>
  {/if}
{/snippet}

{#snippet shift(node: Playlist)}
  {#if lib && !q && manual(node)}
    <span class="shift">
      <span
        role="button"
        tabindex="-1"
        class:off={!app.canShiftPlaylist(node.id, -1)}
        title="Posunout nahoru (⌥↑)"
        onclick={(e) => (e.stopPropagation(), app.shiftPlaylist(node.id, -1))}
        onkeydown={() => {}}
      ><ArrowUp size={12} /></span>
      <span
        role="button"
        tabindex="-1"
        class:off={!app.canShiftPlaylist(node.id, 1)}
        title="Posunout dolů (⌥↓)"
        onclick={(e) => (e.stopPropagation(), app.shiftPlaylist(node.id, 1))}
        onkeydown={() => {}}
      ><ArrowDown size={12} /></span>
    </span>
  {/if}
{/snippet}

{#snippet orderButton(which: "sets" | "lists")}
  <button
    class="add"
    class:sorted={order[which] !== "manual"}
    onclick={(e) => openOrderMenu(e, which)}
    title="Řadit: {ORDERS.find(([o]) => o === order[which])?.[1]}"
  ><ArrowDownUp size={13} /></button>
{/snippet}

<nav class="tree" class:filedrop={fileDrag} bind:this={nav}>
  {#if fileDrag || app.addingFolders}
    <div class="dropnote">
      {#if app.addingFolders}
        Načítám složky a tagy skladeb…
      {:else if fileTarget != null}
        {@const t = app.library?.playlists.find((p) => p.id === fileTarget)}
        {t?.isFolder ? `Složky půjdou do „${t.name}“` : `Skladby přidám do „${t?.name}“, složky vedle`}
      {:else}
        Pusť složky: z každé bude playlist
      {/if}
    </div>
  {/if}
  <label class="search">
    <Search size={14} />
    <input type="search" placeholder="Hledat playlist" bind:value={app.playlistFilter} />
  </label>

  {#if lib}
    <button
      class="item all"
      class:selected={app.selection?.kind === "all" && app.view === "tracks"}
      onclick={() => app.select({ kind: "all" })}
    >
      <Library size={14} class="icon" />
      <span class="name">Celá knihovna</span>
      <span class="readout n">{app.data?.tracks.length ?? 0}</span>
    </button>

    <div class="head">
      <span>Sety</span>
      <span class="tools">
        {@render orderButton("sets")}
        <button class="add" onclick={() => app.createPlaylist(0, "set")} title="Nový set"><ListPlus size={13} /></button>
      </span>
    </div>
    {#each sets.filter((s) => !q || s.name.toLowerCase().includes(q)) as s (s.id)}
      {@render item(s, 0)}
    {/each}
    {#if sets.length === 0}
      <button class="hint" onclick={() => app.createPlaylist(0, "set")}>Vytvoř si první set a přetáhni do něj skladby.</button>
    {/if}
  {/if}

  <div class="head">
    <span>Playlisty</span>
    {#if lib}
      <span class="tools">
        {@render orderButton("lists")}
        <button class="add" onclick={() => app.createPlaylist(0, "playlist")} title="Nový playlist"><ListPlus size={13} /></button>
        <button class="add" onclick={() => app.createPlaylist(0, "folder")} title="Nová složka"><FolderPlus size={13} /></button>
      </span>
    {:else}
      <span class="tools">
        {@render orderButton("lists")}
        <span class="readout">{count}</span>
      </span>
    {/if}
  </div>

  <div class="list">
    {#each rows as { node, depth } (node.id)}
      {@render item(node, depth)}
    {/each}
  </div>

  {#if (app.data?.history.length ?? 0) > 0}
    <div class="head history"><span>Historie</span><span class="readout">{app.data!.history.length}</span></div>
    {#each app.data!.history as h (h.id)}
      <button class="item" class:selected={selected("history", h.id)} onclick={() => app.select({ kind: "history", id: h.id })}>
        <Clock size={14} class="icon" />
        <span class="name">{h.name}</span>
        <span class="readout n">{h.trackIds.length}</span>
      </button>
    {/each}
  {/if}
</nav>

{#if orderMenu}
  {@const which = orderMenu.which}
  <div class="menu" style:left="{orderMenu.x}px" style:top="{orderMenu.y}px" role="menu">
    <p class="note">Řadit {which === "sets" ? "sety" : "playlisty"}</p>
    {#each ORDERS as [o, label]}
      <button role="menuitemradio" aria-checked={order[which] === o} onclick={() => ((order[which] = o), (orderMenu = null))}>
        <span class="check">{#if order[which] === o}<Check size={12} />{/if}</span>{label}
      </button>
    {/each}
    {#if lib && order[which] !== "manual"}
      <div class="sep"></div>
      <button role="menuitem" onclick={() => keepOrder(which)}>Uložit jako vlastní pořadí</button>
    {/if}
  </div>
{/if}

{#if menu}
  {@const node = menu.node}
  {@const tracks = app.playlistTracks(node.id)}
  {@const todo = app.unanalyzed(tracks).length}
  {@const redo = app.unanalyzed(tracks, true).length - todo}
  <div class="menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <button role="menuitem" onclick={() => (app.renaming = node.id)}>Přejmenovat</button>
    <button role="menuitem" disabled={!manual(node) || !app.canShiftPlaylist(node.id, -1)} onclick={() => app.shiftPlaylist(node.id, -1)}>
      Posunout nahoru <kbd>⌥↑</kbd>
    </button>
    <button role="menuitem" disabled={!manual(node) || !app.canShiftPlaylist(node.id, 1)} onclick={() => app.shiftPlaylist(node.id, 1)}>
      Posunout dolů <kbd>⌥↓</kbd>
    </button>
    <div class="sep"></div>
    <button role="menuitem" disabled={todo === 0} onclick={() => app.analyzeTracks(tracks)}>
      Analyzovat {todo ? `(${todo})` : ""}<span class="hint">{todo ? "waveform, grid, tónina" : "vše analyzováno"}</span>
    </button>
    {#if redo > 0}
      <button role="menuitem" onclick={() => app.analyzeTracks(tracks, true)}>
        Analyzovat znovu ({redo + todo})<span class="hint">mimo analýzy z rekordboxu</span>
      </button>
    {/if}
    <div class="sep"></div>
    {#if node.isFolder}
      <button role="menuitem" onclick={() => app.createPlaylist(node.id, "playlist")}>Nový playlist ve složce</button>
      <button role="menuitem" onclick={() => app.createPlaylist(node.id, "folder")}>Nová složka ve složce</button>
    {:else if !node.isSet}
      <button role="menuitem" onclick={() => app.createPlaylist(0, "set").then(() => {
        const id = app.renaming;
        if (id != null) app.setPlaylistTracks(id, node.trackIds);
      })}>Udělat z něj set</button>
    {/if}
    <button role="menuitem" class="danger" onclick={() => app.deletePlaylist(node.id)}>
      Smazat{node.isFolder ? " složku i obsah" : ""}
    </button>
    <p class="note">Skladby z knihovny se nemažou.</p>
  </div>
{/if}

<style>
  .tree {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 0;
    min-height: 0;
  }
  .tree {
    position: relative;
  }
  .tree.filedrop {
    outline: 2px dashed rgb(242 163 58 / 0.6);
    outline-offset: -6px;
    background: rgb(242 163 58 / 0.04);
  }
  .dropnote {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 14px;
    z-index: 5;
    padding: 8px 10px;
    border-radius: 7px;
    background: var(--cue);
    color: var(--booth);
    font-size: 12px;
    font-weight: 600;
    text-align: center;
    pointer-events: none;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 12px;
    padding: 0 10px;
    height: 32px;
    border: 1px solid transparent;
    border-radius: var(--r-control);
    background: var(--surface-2);
    color: var(--text-3);
  }
  .search:focus-within {
    border-color: var(--cue);
  }
  .search input {
    border: 0;
    padding: 0;
    height: 30px;
    flex: 1;
    background: transparent;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 16px 10px 6px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .head .readout {
    font-weight: 500;
    font-size: 12px;
  }
  .head.history {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--hairline);
  }
  .tools {
    display: flex;
    gap: 2px;
  }
  .add {
    border: 0;
    background: none;
    color: var(--text-2);
    padding: 2px 4px;
    border-radius: 4px;
  }
  .add.sorted {
    color: var(--cue);
  }
  .add:hover {
    color: var(--cue);
    background: var(--panel-2);
  }
  .hint {
    margin: 0 16px;
    padding: 6px 0;
    border: 0;
    background: none;
    text-align: left;
    font-size: 12px;
    color: var(--text-3);
  }
  .hint:hover {
    color: var(--text-2);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding-right: 10px;
    padding-left: 10px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    text-align: left;
    font-size: 13px;
    color: var(--text-2);
  }
  .item:hover {
    background: rgb(255 255 255 / 0.03);
  }
  .item.all {
    margin-bottom: 4px;
  }
  .item :global(.icon) {
    color: var(--text-3);
    flex: none;
  }
  .item :global(.icon.set) {
    color: var(--cue);
  }
  .item.selected {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
  }
  .item.selected .n {
    color: var(--accent);
  }
  .item.selected :global(.icon) {
    color: var(--cue);
  }
  .item.drop {
    outline: 1px solid var(--cue);
    outline-offset: -1px;
    background: rgb(242 163 58 / 0.1);
  }
  .item.lifted {
    opacity: 0.4;
  }
  .item.before::after,
  .item.after::after {
    content: "";
    position: absolute;
    left: 12px;
    right: 12px;
    height: 2px;
    border-radius: 1px;
    background: var(--cue);
    pointer-events: none;
  }
  .item.before::after {
    top: -1px;
  }
  .item.after::after {
    bottom: -1px;
  }
  .shift {
    display: none;
    gap: 1px;
    margin-right: -8px;
  }
  .item:hover .shift {
    display: flex;
  }
  .item:hover .n {
    display: none;
  }
  .shift span {
    display: flex;
    padding: 2px 3px;
    border-radius: 4px;
    color: var(--text-2);
  }
  .shift span:hover {
    color: var(--cue);
    background: rgb(255 255 255 / 0.06);
  }
  .shift span.off {
    visibility: hidden;
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  .rename {
    flex: 1;
    height: 22px;
    font-size: 13px;
  }
  .menu {
    position: fixed;
    min-width: 200px;
    padding: 6px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
    z-index: 30;
  }
  .menu button {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 13px;
  }
  .menu button:hover:not(:disabled) {
    background: var(--panel-2);
  }
  .menu button:disabled {
    color: var(--text-3);
  }
  .menu kbd {
    float: right;
    font-family: inherit;
    font-size: 11px;
    color: var(--text-3);
  }
  .menu .danger {
    color: #f2a3a5;
  }
  .menu .hint {
    float: right;
    margin-left: 12px;
    font-size: 11px;
    color: var(--text-3);
  }
  .menu .check {
    display: inline-flex;
    width: 18px;
    color: var(--cue);
    vertical-align: -1px;
  }
  .menu .sep {
    height: 1px;
    margin: 5px 4px;
    background: var(--line);
  }
  .menu .note {
    margin: 4px 10px 2px;
    font-size: 11px;
    color: var(--text-3);
  }
</style>
