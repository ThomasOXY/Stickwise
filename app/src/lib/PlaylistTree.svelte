<script lang="ts">
  import { ChevronDown, ChevronRight, Clock, ListMusic, Search } from "@lucide/svelte";
  import type { Playlist } from "./api";
  import { app } from "./state.svelte";

  let collapsed = $state<Record<number, boolean>>({});

  const q = $derived(app.playlistFilter.trim().toLowerCase());
  const count = $derived(app.data?.playlists.filter((p) => !p.isFolder).length ?? 0);

  interface Row {
    node: Playlist;
    depth: number;
  }

  /** Flattened, filtered tree in display order. */
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const seen = new Set<number>();
    const walk = (parent: number, depth: number) => {
      for (const n of app.sortedChildren(parent)) {
        if (seen.has(n.id)) continue;
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
</script>

<nav class="tree">
  <label class="search">
    <Search size={14} />
    <input type="search" placeholder="Hledat playlist" bind:value={app.playlistFilter} />
  </label>

  <div class="head"><span>Playlisty</span><span class="readout">{count}</span></div>

  <div class="list">
    {#each rows as { node, depth } (node.id)}
      {#if node.isFolder}
        <button class="item folder" style:padding-left="{8 + depth * 18}px" onclick={() => (collapsed[node.id] = !collapsed[node.id])}>
          {#if collapsed[node.id]}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
          <span class="name">{node.name}</span>
        </button>
      {:else}
        <button
          class="item"
          class:selected={selected("playlist", node.id)}
          style:padding-left="{8 + depth * 18 + (depth > 0 ? 4 : 0)}px"
          onclick={() => app.select({ kind: "playlist", id: node.id })}
        >
          <ListMusic size={14} class="icon" />
          <span class="name">{node.name}</span>
          <span class="readout n">{node.trackIds.length}</span>
        </button>
      {/if}
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
    margin: 0 16px 6px;
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
  .item.folder {
    color: var(--text);
  }
  .item :global(.icon) {
    color: var(--text-2);
    flex: none;
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
</style>
