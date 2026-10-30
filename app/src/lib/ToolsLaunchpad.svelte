<script lang="ts">
  import {
    AudioWaveform,
    BrainCircuit,
    Download,
    Eraser,
    HardDriveDownload,
    LibraryBig,
    RefreshCw,
    Route,
    Save,
    Search,
    ShieldCheck,
    SlidersHorizontal,
    Sparkles,
  } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { app } from "./state.svelte";

  interface Tool {
    name: string;
    hint: string;
    icon: Component;
    /** Tile gradient. */
    from: string;
    to: string;
    /** Why it cannot run now; empty when it can. */
    blocked: string;
    run: () => void | Promise<void>;
  }

  let query = $state("");
  let focus = $state(0);

  const firstStick = $derived(app.sticks[0]?.path);
  const rekordbox = $derived(app.library?.rekordboxSources[0]);
  const libTracks = $derived(app.library?.tracks.length ?? 0);
  const notAnalyzed = $derived(app.unanalyzed((app.library?.tracks ?? []).map((t) => t.id)));

  /** Open a stick unless one already is. */
  async function onStick() {
    if (!app.isLibrary && app.stickPath) return;
    if (firstStick) await app.open(firstStick);
  }

  async function onLibrary() {
    if (!app.isLibrary) await app.openLibrary();
  }

  const tools = $derived<Tool[]>([
    {
      name: "Knihovna",
      hint: "Sety, playlisty a skladby na tomhle Macu",
      icon: LibraryBig,
      from: "#3a4150",
      to: "#1f242e",
      blocked: "",
      run: async () => {
        await onLibrary();
        app.view = "tracks";
      },
    },
    {
      name: "Synchronizace",
      hint: "Sety a playlisty z knihovny na flashku",
      icon: RefreshCw,
      from: "#f2a33a",
      to: "#c46a12",
      blocked: libTracks === 0 ? "Knihovna je prázdná" : "",
      run: () => {
        app.syncOpen = true;
      },
    },
    {
      name: "Plánovač setu",
      hint: "Pořadí podle tóniny, tempa a energie, přechody",
      icon: Route,
      from: "#9b5cf0",
      to: "#5b2bb5",
      blocked: libTracks === 0 ? "Knihovna je prázdná" : "",
      run: async () => {
        await onLibrary();
        const p = app.currentPlaylist?.trackIds.length ? app.currentPlaylist : app.data?.playlists.find((x) => x.isSet && x.trackIds.length > 1);
        if (!p) {
          app.notice = "Vyber set nebo playlist s aspoň dvěma skladbami, pak otevři plánovač.";
          return;
        }
        app.select({ kind: "playlist", id: p.id });
        app.view = "planner";
      },
    },
    {
      name: "Analýza skladeb",
      hint: notAnalyzed.length
        ? `${notAnalyzed.length} skladeb bez waveformu a gridu`
        : "Waveform, beatgrid a tónina pro nové skladby",
      icon: AudioWaveform,
      from: "#59c98a",
      to: "#1f7a8c",
      blocked: app.analyzeJob != null ? "Už běží" : notAnalyzed.length === 0 ? "Všechno je analyzované" : "",
      run: async () => {
        await onLibrary();
        app.analyzeTracks(notAnalyzed);
      },
    },
    {
      name: "Smart Cue",
      hint: "Dropy, breaky a další místa pro cue",
      icon: Sparkles,
      from: "#ee5fb0",
      to: "#a8287a",
      blocked: app.deck ? "" : "Nejdřív vyber skladbu",
      run: () => app.loadSmart(),
    },
    {
      name: "AI analýza cue",
      hint: "CUE-DETR projde skladby v otevřeném seznamu",
      icon: BrainCircuit,
      from: "#3fcfd4",
      to: "#16808a",
      blocked: !app.aiInstalled
        ? "AI analýza není k dispozici"
        : app.aiJob != null
          ? "Už běží"
          : app.currentTrackIds.length === 0
            ? "Otevři playlist"
            : "",
      run: () => app.aiAnalyze(app.currentTrackIds),
    },
    {
      name: "Kontrola flashky",
      hint: "Co na kterém přehrávači nepůjde a proč",
      icon: ShieldCheck,
      from: "#59c98a",
      to: "#21804a",
      blocked: firstStick ? "" : "Vlož flashku",
      run: async () => {
        await onStick();
        app.view = "health";
        if (!app.verify) app.runVerify();
      },
    },
    {
      name: "Zapsat cue",
      hint: "Upravené cue z aplikace na flashku",
      icon: Save,
      from: "#e6d23f",
      to: "#a8901a",
      blocked: app.isLibrary || !app.stickPath ? "Otevři flashku" : app.pending.size === 0 ? "Žádné neuložené cue" : "",
      run: () => {
        app.writeOpen = true;
      },
    },
    {
      name: "Export na disk",
      hint: "Playlisty z flashky do složek",
      icon: HardDriveDownload,
      from: "#3e7bf2",
      to: "#1d47a8",
      blocked: firstStick ? "" : "Vlož flashku",
      run: async () => {
        await onStick();
        app.exportOpen = true;
      },
    },
    {
      name: "Nastavení přehrávače",
      hint: "My Settings pro CDJ/XDJ a mixer",
      icon: SlidersHorizontal,
      from: "#8b909c",
      to: "#4a4f5a",
      blocked: "",
      run: () => {
        app.settingsFor = app.isLibrary ? null : app.stickPath;
      },
    },
    {
      name: "Import z rekordboxu",
      hint: "Skladby, playlisty a cue, tvoje úpravy zůstanou",
      icon: Download,
      from: "#e8423c",
      to: "#9c1d19",
      blocked: !rekordbox ? "Rekordbox knihovna nenalezena" : app.importing ? "Už běží" : "",
      run: async () => {
        await onLibrary();
        if (rekordbox) app.importRekordbox(rekordbox);
      },
    },
    {
      name: "Formátovat flashku",
      hint: "Smazat a připravit pro přehrávače",
      icon: Eraser,
      from: "#c93a3f",
      to: "#6e1a1d",
      blocked: "",
      run: () => {
        app.formatOpen = true;
      },
    },
  ]);

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? tools.filter((t) => `${t.name} ${t.hint}`.toLowerCase().includes(q)) : tools;
  });

  $effect(() => {
    void query;
    focus = 0;
  });

  function close() {
    app.toolsOpen = false;
  }

  async function launch(t: Tool) {
    if (t.blocked) return;
    close();
    try {
      await t.run();
    } catch (e) {
      app.error = String(e);
    }
  }

  const COLS = 6;
  let grid = $state<HTMLElement>();
  /** Columns the grid shows now (fewer in a narrow window). */
  const cols = () => (grid ? getComputedStyle(grid).gridTemplateColumns.split(" ").length : COLS);

  function onKey(e: KeyboardEvent) {
    const n = shown.length;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Enter" && shown[focus]) {
      e.preventDefault();
      launch(shown[focus]);
    } else if (n && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
      e.preventDefault();
      focus = (focus + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
    } else if (n && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      const next = focus + (e.key === "ArrowDown" ? cols() : -cols());
      if (next >= 0 && next < n) focus = next;
    }
  }

  function autofocus(el: HTMLInputElement) {
    el.focus();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="pad" role="dialog" aria-modal="true" aria-label="Nástroje" tabindex="-1" onclick={(e) => !(e.target as HTMLElement).closest(".search, .tool") && close()} onkeydown={onKey}>
  <label class="search">
    <Search size={15} />
    <input use:autofocus type="search" placeholder="Hledat nástroj" bind:value={query} />
  </label>

  <div class="grid" bind:this={grid} style:--cols={COLS}>
    {#each shown as t, i (t.name)}
      <button
        class="tool"
        class:blocked={!!t.blocked}
        class:focus={i === focus}
        style:--from={t.from}
        style:--to={t.to}
        style:animation-delay="{i * 18}ms"
        title={t.blocked || t.hint}
        aria-disabled={!!t.blocked}
        onclick={() => launch(t)}
        onpointerenter={() => (focus = i)}
      >
        <span class="icon"><t.icon size={38} strokeWidth={1.6} /></span>
        <span class="name">{t.name}</span>
        <span class="hint">{t.blocked || t.hint}</span>
      </button>
    {/each}
    {#if shown.length === 0}
      <p class="none">Žádný nástroj neodpovídá „{query}“.</p>
    {/if}
  </div>

  <p class="foot">Esc zavře · šipky a Enter vyberou · ⌘K otevře kdykoli</p>
</div>

<style>
  .pad {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 56px 48px 28px;
    background: rgb(14 16 20 / 0.72);
    backdrop-filter: blur(28px) saturate(1.3);
    -webkit-backdrop-filter: blur(28px) saturate(1.3);
    animation: appear 160ms ease-out;
    outline: none;
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: scale(1.04);
    }
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 260px;
    height: 32px;
    padding: 0 12px;
    border-radius: 9px;
    border: 1px solid rgb(255 255 255 / 0.12);
    background: rgb(255 255 255 / 0.07);
    color: var(--text-2);
  }
  .search:focus-within {
    border-color: rgb(242 163 58 / 0.6);
  }
  .search input {
    flex: 1;
    height: 28px;
    padding: 0;
    border: 0;
    background: transparent;
    font-size: 14px;
  }
  .grid {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(var(--cols), 150px);
    align-content: center;
    gap: 34px 26px;
    max-width: 100%;
  }
  .tool {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 6px 10px;
    border: 0;
    border-radius: 18px;
    background: none;
    animation: rise 260ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.94);
    }
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 92px;
    height: 92px;
    border-radius: 22px;
    background: linear-gradient(160deg, var(--from), var(--to));
    color: #fff;
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.25),
      0 10px 24px rgb(0 0 0 / 0.4);
    transition: transform 140ms ease;
  }
  .tool.focus .icon {
    transform: scale(1.07);
  }
  .tool:active .icon {
    transform: scale(0.96);
  }
  .tool.focus {
    background: rgb(255 255 255 / 0.05);
  }
  .name {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    text-align: center;
  }
  .hint {
    min-height: 30px;
    font-size: 11.5px;
    line-height: 1.3;
    color: var(--text-2);
    text-align: center;
  }
  .tool.blocked {
    cursor: default;
  }
  .tool.blocked .icon {
    filter: grayscale(0.85) brightness(0.55);
    box-shadow: none;
  }
  .tool.blocked .name {
    color: var(--text-2);
  }
  .tool.blocked .hint {
    color: var(--text-3);
  }
  .none {
    grid-column: 1 / -1;
    color: var(--text-2);
    text-align: center;
  }
  .foot {
    margin: 0;
    font-size: 11.5px;
    color: var(--text-3);
  }
  @media (max-width: 1180px) {
    .grid {
      grid-template-columns: repeat(4, 150px);
    }
  }
</style>
