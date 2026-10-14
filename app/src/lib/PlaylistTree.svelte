<script lang="ts">
  import { ChevronDown, ChevronRight, Clock, FolderPlus, Library, ListMusic, ListPlus, Search, Sparkles } from "@lucide/svelte";
  import type { Playlist } from "./api";
  import { app } from "./state.svelte";

  let collapsed = $state<Record<number, boolean>>({});
  let menu = $state<{ node: Playlist; x: number; y: number } | null>(null);
  let dropTarget = $state<number | null>(null);
  let renameValue = $state("");

  const q = $derived(app.playlistFilter.trim().toLowerCase());
  const lib = $derived(app.isLibrary);
  const sets = $derived((app.data?.playlists ?? []).filter((p) => p.isSet).sort((a, b) => a.sortOrder - b.sortOrder));
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
      for (const n of app.sortedChildren(parent)) {
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

  // Tracks dragged from the table land in a playlist.
  function dragOver(e: DragEvent, node: Playlist) {
    if (!lib || node.isFolder || !e.dataTransfer?.types.includes("application/x-rbx-tracks")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    dropTarget = node.id;
  }

  function drop(e: DragEvent, node: Playlist) {
    dropTarget = null;
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

<svelte:window onclick={() => (menu = null)} />

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
      class="item folder"
      style:padding-left="{8 + depth * 18}px"
      onclick={() => (collapsed[node.id] = !collapsed[node.id])}
      oncontextmenu={(e) => openMenu(e, node)}
    >
      {#if collapsed[node.id]}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
      <span class="name">{node.name}</span>
    </button>
  {:else}
    <button
      class="item"
      class:selected={selected("playlist", node.id)}
      class:drop={dropTarget === node.id}
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
    </button>
  {/if}
{/snippet}

<nav class="tree">
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
      <button class="add" onclick={() => app.createPlaylist(0, "set")} title="Nový set"><ListPlus size={13} /></button>
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
        <button class="add" onclick={() => app.createPlaylist(0, "playlist")} title="Nový playlist"><ListPlus size={13} /></button>
        <button class="add" onclick={() => app.createPlaylist(0, "folder")} title="Nová složka"><FolderPlus size={13} /></button>
      </span>
    {:else}
      <span class="readout">{count}</span>
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

{#if menu}
  {@const node = menu.node}
  <div class="menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <button role="menuitem" onclick={() => (app.renaming = node.id)}>Přejmenovat</button>
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
    width: 260px;
    flex: none;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--line);
    padding: 12px 0 12px;
    min-height: 0;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 12px 14px;
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
    padding: 0;
    height: 26px;
    flex: 1;
    background: transparent;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 12px 16px 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .head .readout {
    font-weight: 500;
    font-size: 12px;
  }
  .head.history {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
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
    gap: 8px;
    width: 100%;
    height: 26px;
    padding-right: 16px;
    padding-left: 16px;
    border: 0;
    background: none;
    text-align: left;
    font-size: 13px;
  }
  .item:hover {
    background: rgb(255 255 255 / 0.03);
  }
  .item.all {
    margin-bottom: 4px;
  }
  .item :global(.icon) {
    color: var(--text-2);
    flex: none;
  }
  .item :global(.icon.set) {
    color: var(--cue);
  }
  .item.selected {
    background: var(--panel-2);
  }
  .item.selected::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--cue);
  }
  .item.selected :global(.icon) {
    color: var(--cue);
  }
  .item.drop {
    outline: 1px solid var(--cue);
    outline-offset: -1px;
    background: rgb(242 163 58 / 0.1);
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
  .menu button:hover {
    background: var(--panel-2);
  }
  .menu .danger {
    color: #f2a3a5;
  }
  .menu .note {
    margin: 4px 10px 2px;
    font-size: 11px;
    color: var(--text-3);
  }
</style>
