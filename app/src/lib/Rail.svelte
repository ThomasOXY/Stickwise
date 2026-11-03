<script lang="ts">
  import { Activity, ArrowDownToLine, Command, Disc3, LibraryBig, Route, Settings, ShieldCheck, Usb, WandSparkles } from "@lucide/svelte";
  import { openSettings } from "./prefs.svelte";
  import { app } from "./state.svelte";
  import { updates } from "./updates.svelte";

  const firstStick = $derived(app.stickPath ?? app.sticks[0]?.path ?? null);
  const running = $derived(app.jobs.filter((j) => j.state === "running").length);

  const onLibrary = $derived(app.isLibrary && app.view === "tracks");
  const onPlanner = $derived(app.view === "planner");
  const onStick = $derived(!app.isLibrary && app.view === "tracks");
  const onHealth = $derived(!app.isLibrary && app.view === "health");

  async function library() {
    if (!app.isLibrary) await app.openLibrary();
    app.view = "tracks";
  }

  async function planner() {
    if (!app.isLibrary) await app.openLibrary();
    const p = app.currentPlaylist?.trackIds.length ? app.currentPlaylist : app.data?.playlists.find((x) => x.isSet && x.trackIds.length > 1);
    if (!p) {
      app.notice = "Vyber set nebo playlist s aspoň dvěma skladbami, pak otevři plánovač.";
      return;
    }
    app.select({ kind: "playlist", id: p.id });
    app.view = "planner";
  }

  async function stick(health: boolean) {
    if (!firstStick) {
      app.notice = "Vlož flashku.";
      return;
    }
    if (app.isLibrary || app.stickPath !== firstStick) await app.open(firstStick);
    app.view = health ? "health" : "tracks";
    if (health && !app.verify) app.runVerify();
  }

  function smart() {
    if (!app.deck) {
      app.notice = "Nejdřív vyber skladbu.";
      return;
    }
    app.smartOpen ? (app.smartOpen = false) : app.loadSmart();
  }
</script>

<nav class="rail" data-tauri-drag-region>
  <div class="logo" data-tauri-drag-region><Disc3 size={20} /></div>
  <button class:on={onLibrary} onclick={library} title="Knihovna"><LibraryBig size={18} /></button>
  <button class:on={onPlanner} onclick={planner} title="Plánovač setu"><Route size={18} /></button>
  <button class:on={app.smartOpen} onclick={smart} title="Smart Cue"><WandSparkles size={18} /></button>
  <button class:on={onStick} onclick={() => stick(false)} title="Flashka" disabled={!firstStick}><Usb size={18} /></button>
  <button class:on={onHealth} onclick={() => stick(true)} title="Kontrola flashky" disabled={!firstStick}><ShieldCheck size={18} /></button>
  <div class="spacer" data-tauri-drag-region></div>
  {#if updates.available}
    <button class="update" onclick={() => (updates.open = true)} title="Nová verze {updates.update?.version}">
      <ArrowDownToLine size={18} />
      <span class="pip"></span>
    </button>
  {/if}
  <button class="jobs" class:busy={running > 0} title={running ? `Běží ${running} úloh` : "Nic neběží"} onclick={() => (app.toolsOpen = true)}>
    <Activity size={18} />
    {#if running}<span class="badge readout">{running}</span>{/if}
  </button>
  <button onclick={() => openSettings()} title="Nastavení (⌘,)"><Settings size={18} /></button>
  <button class="cmdk" onclick={() => (app.toolsOpen = true)} title="Všechny nástroje (⌘K)"><Command size={16} /></button>
</nav>

<style>
  .rail {
    width: 68px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    /* Room for the macOS traffic lights above the logo. */
    padding: 44px 0 12px;
  }
  .logo {
    width: 36px;
    height: 36px;
    margin-bottom: 18px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--on-accent);
    background: linear-gradient(200deg, #f7b85a, #e08a1e);
  }
  button {
    position: relative;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: var(--text-3);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  button:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  button.on {
    color: var(--accent);
    background: var(--surface-3);
  }
  button:disabled {
    opacity: 0.35;
    pointer-events: none;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .jobs.busy {
    color: var(--accent);
  }
  .badge {
    position: absolute;
    top: 4px;
    right: 3px;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    line-height: 16px;
  }
  .update {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .pip {
    position: absolute;
    top: 7px;
    right: 7px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--bg);
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  .cmdk {
    border: 1px solid var(--border);
  }
</style>
