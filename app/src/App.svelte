<script lang="ts">
  import { onMount } from "svelte";
  import Deck from "./lib/Deck.svelte";
  import EmptyState from "./lib/EmptyState.svelte";
  import ExportSheet from "./lib/ExportSheet.svelte";
  import FormatSheet from "./lib/FormatSheet.svelte";
  import HealthReport from "./lib/HealthReport.svelte";
  import LibraryHeader from "./lib/LibraryHeader.svelte";
  import SetPlanner from "./lib/SetPlanner.svelte";
  import SyncSheet from "./lib/SyncSheet.svelte";
  import PlayerSettingsSheet from "./lib/PlayerSettingsSheet.svelte";
  import Rail from "./lib/Rail.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import StickHeader from "./lib/StickHeader.svelte";
  import TrackDetail from "./lib/TrackDetail.svelte";
  import ToolsLaunchpad from "./lib/ToolsLaunchpad.svelte";
  import TrackTable from "./lib/TrackTable.svelte";
  import UpdateSheet from "./lib/UpdateSheet.svelte";
  import AccessSheet from "./lib/AccessSheet.svelte";
  import PeerSheet from "./lib/PeerSheet.svelte";
  import PairPrompt from "./lib/PairPrompt.svelte";
  import OfferPrompt from "./lib/OfferPrompt.svelte";
  import MirrorSheet from "./lib/MirrorSheet.svelte";
  import WriteCuesSheet from "./lib/WriteCuesSheet.svelte";
  import { app } from "./lib/state.svelte";
  import { updates } from "./lib/updates.svelte";
  import { access } from "./lib/access.svelte";
  import { prefs } from "./lib/prefs.svelte";
  import { listen } from "@tauri-apps/api/event";

  let deck = $state<ReturnType<typeof Deck>>();

  onMount(() => {
    app.init();
    updates.init();
    access.init();
    prefs.init().catch((e) => (app.error = String(e)));
    // "Zkontrolovat aktualizace" in the Settings window.
    const off = listen("open-updates", () => {
      updates.open = true;
      if (!updates.available) updates.check();
    });
    return () => off.then((f) => f());
  });

  // Helpers switched off in the settings: close what is open of them.
  $effect(() => {
    if (prefs.value.general.helpers) return;
    app.smartOpen = false;
    if (app.view === "planner") app.view = "tracks";
  });

  function onKey(e: KeyboardEvent) {
    // ⌘K or F4 (the Launchpad key) open the tools, anywhere.
    if ((e.metaKey && e.key.toLowerCase() === "k") || e.key === "F4") {
      e.preventDefault();
      app.toolsOpen = !app.toolsOpen;
      return;
    }
    const typing = (e.target as HTMLElement)?.tagName === "INPUT";
    // ⌘S sync, ⌘P plan the open set: the shortcuts the tools palette shows.
    if (e.metaKey && !e.shiftKey && !e.altKey && !typing) {
      const k = e.key.toLowerCase();
      if (k === "s" && (app.library?.tracks.length ?? 0) > 0) {
        e.preventDefault();
        app.toolsOpen = false;
        app.syncOpen = true;
        return;
      }
      if (k === "p" && prefs.value.general.helpers && app.isLibrary && (app.currentPlaylist?.trackIds.length ?? 0) > 1) {
        e.preventDefault();
        app.toolsOpen = false;
        app.view = "planner";
        return;
      }
    }
    if (typing || app.toolsOpen || app.exportOpen || app.writeOpen || app.syncOpen || app.formatOpen || app.peerOpen || app.pairRequest || app.offerRequest || app.mirrorRequest || updates.open || access.open || app.settingsFor !== undefined) return;
    if (e.code === "Space") {
      e.preventDefault();
      deck?.toggle();
    } else if (e.key === "+" || e.key === "=") {
      deck?.zoom(1 / 1.5);
    } else if (e.key === "-") {
      deck?.zoom(1.5);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="window">
  <Rail />
  <Sidebar />
  <main>
    {#if app.data}
      {#if app.isLibrary}<LibraryHeader />{:else}<StickHeader />{/if}
      <div class="content">
        {#if app.view === "health"}
          <HealthReport />
        {:else if app.view === "planner"}
          <SetPlanner />
        {:else}
          <TrackTable />
          {#if app.detailOpen}<TrackDetail />{/if}
        {/if}
      </div>
      {#if app.error}
        <button class="toast" onclick={() => (app.error = null)}>{app.error}</button>
      {:else if app.notice}
        <button class="toast ok" onclick={() => (app.notice = null)}>{app.notice}</button>
      {/if}
    {:else if app.loading}
      <div class="loading" data-tauri-drag-region>{app.isLibrary ? "Načítám knihovnu…" : "Načítám knihovnu z flashky…"}</div>
    {:else}
      <EmptyState />
    {/if}
    <!-- Outside the source views: switching library / stick does not stop playback. -->
    {#if app.data || app.deck}<Deck bind:this={deck} />{/if}
  </main>
</div>

{#if app.exportOpen && app.data}
  <ExportSheet />
{/if}
{#if app.writeOpen && app.data}
  <WriteCuesSheet />
{/if}
{#if app.syncOpen}
  <SyncSheet />
{/if}
{#if app.settingsFor}
  <PlayerSettingsSheet stick={app.settingsFor} />
{/if}
{#if app.formatOpen}
  <FormatSheet />
{/if}
{#if app.peerOpen}
  <PeerSheet />
{/if}
{#if app.toolsOpen}
  <ToolsLaunchpad />
{/if}
{#if updates.open}
  <UpdateSheet />
{:else if access.open}
  <AccessSheet />
{/if}
{#if app.pairRequest}
  <PairPrompt />
{:else if app.offerRequest}
  <OfferPrompt />
{:else if app.mirrorRequest}
  <MirrorSheet />
{/if}

<style>
  .window {
    display: flex;
    height: 100%;
    background: var(--bg);
  }
  main {
    position: relative;
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    margin: 10px 10px 10px 0;
    border-radius: 16px;
    border: 1px solid var(--hairline);
    background: var(--surface-1);
    overflow: hidden;
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
    top: 18px;
    transform: translateX(-50%);
    max-width: 640px;
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid rgb(229 72 77 / 0.45);
    background: #2a1a1c;
    color: #f2c4c5;
    font-size: 12px;
    text-align: left;
    z-index: 15;
  }
  .toast.ok {
    border-color: rgb(89 201 138 / 0.45);
    background: #17251d;
    color: #bfe8cf;
  }
</style>
