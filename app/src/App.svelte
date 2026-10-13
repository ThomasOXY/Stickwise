<script lang="ts">
  import { onMount } from "svelte";
  import Deck from "./lib/Deck.svelte";
  import EmptyState from "./lib/EmptyState.svelte";
  import ExportSheet from "./lib/ExportSheet.svelte";
  import HealthReport from "./lib/HealthReport.svelte";
  import PlaylistTree from "./lib/PlaylistTree.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import StickHeader from "./lib/StickHeader.svelte";
  import TrackDetail from "./lib/TrackDetail.svelte";
  import TrackTable from "./lib/TrackTable.svelte";
  import { app } from "./lib/state.svelte";

  let deck = $state<ReturnType<typeof Deck>>();

  onMount(() => {
    app.init();
  });

  function onKey(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.tagName === "INPUT";
    if (e.code === "Space" && !typing && !app.exportOpen) {
      e.preventDefault();
      deck?.toggle();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="window">
  <Sidebar />
  <main>
    {#if app.data}
      <StickHeader />
      <div class="content">
        <PlaylistTree />
        {#if app.view === "health"}
          <HealthReport />
        {:else}
          <TrackTable />
          {#if app.detailOpen}<TrackDetail />{/if}
        {/if}
      </div>
      <Deck bind:this={deck} />
      {#if app.error}
        <button class="toast" onclick={() => (app.error = null)}>{app.error}</button>
      {/if}
    {:else if app.loading}
      <div class="loading" data-tauri-drag-region>Načítám knihovnu z flashky…</div>
    {:else}
      <EmptyState />
    {/if}
  </main>
</div>

{#if app.exportOpen && app.data}
  <ExportSheet />
{/if}

<style>
  .window {
    display: flex;
    height: 100%;
  }
  main {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--booth);
  }
  .content {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .loading {
    margin: auto;
    color: var(--text-2);
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: 136px;
    transform: translateX(-50%);
    max-width: 640px;
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid rgb(229 72 77 / 0.45);
    background: #2a1a1c;
    color: #f2c4c5;
    font-size: 12px;
    text-align: left;
  }
</style>
