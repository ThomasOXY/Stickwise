<script lang="ts">
  import {
    ArrowDownToLine,
    AudioWaveform,
    BrainCircuit,
    CornerDownLeft,
    Download,
    Eraser,
    HardDriveDownload,
    LibraryBig,
    Laptop,
    RefreshCw,
    Route,
    Save,
    Search,
    ShieldCheck,
    SlidersHorizontal,
    Settings,
    Sparkles,
  } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { openSettings, prefs } from "./prefs.svelte";
  import { t as tr, type Key } from "./i18n.svelte";
  import { app } from "./state.svelte";
  import { updates } from "./updates.svelte";

  type Group = "library" | "stick" | "app";

  interface Tool {
    /** Stable id, kept in the recent list. */
    id: string;
    name: string;
    hint: string;
    icon: Component;
    /** Tint of the icon tile. */
    tone: string;
    group: Group;
    /** ⌘ + this key launches it while the palette is open. */
    key?: string;
    /** Why it cannot run now; empty when it can. */
    blocked: string;
    /** A helper (Smart Cue, AI, planner): hidden when they are off in the settings. */
    helper?: boolean;
    run: () => void | Promise<void>;
  }

  const RECENT_KEY = "tools-recent";
  let recent = $state<string[]>(JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]"));

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

  const all = $derived<Tool[]>([
    {
      id: "library",
      name: tr("shell.tools.library.name"),
      hint: tr("shell.tools.library.hint"),
      icon: LibraryBig,
      tone: "#a2a7af",
      group: "library",
      blocked: "",
      run: async () => {
        await onLibrary();
        app.view = "tracks";
      },
    },
    {
      id: "sync",
      name: tr("shell.tools.sync.name"),
      hint: tr("shell.tools.sync.hint"),
      icon: RefreshCw,
      tone: "#f2a33a",
      group: "stick",
      key: "S",
      blocked: libTracks === 0 ? tr("shell.tools.blocked.emptyLibrary") : "",
      run: () => {
        app.syncOpen = true;
      },
    },
    {
      id: "planner",
      name: tr("shell.tools.planner.name"),
      helper: true,
      hint: tr("shell.tools.planner.hint"),
      icon: Route,
      tone: "#9b5cf0",
      group: "library",
      key: "P",
      blocked: libTracks === 0 ? tr("shell.tools.blocked.emptyLibrary") : "",
      run: async () => {
        await onLibrary();
        const p = app.currentPlaylist?.trackIds.length ? app.currentPlaylist : app.data?.playlists.find((x) => x.isSet && x.trackIds.length > 1);
        if (!p) {
          app.notice = tr("shell.plannerNeedsSet");
          return;
        }
        app.select({ kind: "playlist", id: p.id });
        app.view = "planner";
      },
    },
    {
      id: "analysis",
      name: tr("shell.tools.analysis.name"),
      hint: notAnalyzed.length
        ? tr("shell.tools.analysis.hintCount", { count: notAnalyzed.length })
        : tr("shell.tools.analysis.hint"),
      icon: AudioWaveform,
      tone: "#59c98a",
      group: "library",
      blocked: notAnalyzed.length === 0 ? tr("shell.tools.blocked.allAnalyzed") : "",
      run: async () => {
        await onLibrary();
        app.analyzeTracks(notAnalyzed);
      },
    },
    {
      id: "smartCue",
      name: "Smart Cue",
      helper: true,
      hint: tr("shell.tools.smartCue.hint"),
      icon: Sparkles,
      tone: "#ee5fb0",
      group: "library",
      blocked: app.deck ? "" : tr("shell.tools.blocked.pickTrack"),
      run: () => app.loadSmart(),
    },
    {
      id: "ai",
      name: tr("shell.tools.ai.name"),
      helper: true,
      hint: tr("shell.tools.ai.hint"),
      icon: BrainCircuit,
      tone: "#3fcfd4",
      group: "library",
      blocked: !app.aiInstalled
        ? tr("shell.tools.blocked.aiMissing")
        : app.aiJob != null
          ? tr("shell.tools.blocked.running")
          : app.currentTrackIds.length === 0
            ? tr("shell.tools.blocked.openPlaylist")
            : "",
      run: () => app.aiAnalyze(app.currentTrackIds),
    },
    {
      id: "verify",
      name: tr("shell.tools.verify.name"),
      hint: tr("shell.tools.verify.hint"),
      icon: ShieldCheck,
      tone: "#59c98a",
      group: "stick",
      blocked: firstStick ? "" : tr("shell.tools.blocked.insertStick"),
      run: async () => {
        await onStick();
        app.view = "health";
        if (!app.verify) app.runVerify();
      },
    },
    {
      id: "writeCues",
      name: tr("shell.tools.writeCues.name"),
      hint: tr("shell.tools.writeCues.hint"),
      icon: Save,
      tone: "#e6d23f",
      group: "stick",
      blocked: app.isLibrary || !app.stickPath ? tr("shell.tools.blocked.openStick") : app.pending.size === 0 ? tr("shell.tools.blocked.noPending") : "",
      run: () => {
        app.writeOpen = true;
      },
    },
    {
      id: "export",
      name: tr("shell.tools.export.name"),
      hint: tr("shell.tools.export.hint"),
      icon: HardDriveDownload,
      tone: "#3e7bf2",
      group: "stick",
      blocked: firstStick ? "" : tr("shell.tools.blocked.insertStick"),
      run: async () => {
        await onStick();
        app.exportOpen = true;
      },
    },
    {
      id: "playerSettings",
      name: tr("shell.tools.playerSettings.name"),
      hint: tr("shell.tools.playerSettings.hint"),
      icon: SlidersHorizontal,
      tone: "#8b909c",
      group: "stick",
      blocked: "",
      run: () => {
        if (app.isLibrary || !app.stickPath) openSettings("player");
        else app.settingsFor = app.stickPath;
      },
    },
    {
      id: "import",
      name: tr("shell.tools.import.name"),
      hint: tr("shell.tools.import.hint"),
      icon: Download,
      tone: "#e8423c",
      group: "library",
      blocked: !rekordbox ? tr("shell.tools.blocked.noRekordbox") : app.importing ? tr("shell.tools.blocked.running") : "",
      run: async () => {
        await onLibrary();
        if (rekordbox) app.importRekordbox(rekordbox);
      },
    },
    {
      id: "format",
      name: tr("shell.tools.format.name"),
      hint: tr("shell.tools.format.hint"),
      icon: Eraser,
      tone: "#c93a3f",
      group: "stick",
      blocked: "",
      run: () => {
        app.formatOpen = true;
      },
    },
    {
      id: "peer",
      name: tr("shell.tools.peer.name"),
      hint: tr("shell.tools.peer.hint"),
      icon: Laptop,
      tone: "#4aa8ff",
      group: "library",
      blocked: "",
      run: () => {
        app.peerOpen = true;
      },
    },
    {
      id: "settings",
      name: tr("shell.tools.settings.name"),
      hint: tr("shell.tools.settings.hint"),
      icon: Settings,
      tone: "#8b909c",
      group: "app",
      blocked: "",
      run: () => openSettings(),
    },
    {
      id: "updates",
      name: tr("shell.tools.updates.name"),
      hint: updates.available
        ? tr("shell.tools.updates.hintReady", { version: updates.update?.version ?? "" })
        : tr("shell.tools.updates.hintCheck", { version: updates.current }),
      icon: ArrowDownToLine,
      tone: "#3fcf8e",
      group: "app",
      blocked: "",
      run: () => {
        updates.open = true;
        if (!updates.available) updates.check();
      },
    },
  ]);
  const tools = $derived(all.filter((t) => !t.helper || prefs.value.general.helpers));

  const matches = (t: Tool, q: string) => !q || `${t.name} ${t.hint}`.toLowerCase().includes(q);

  /** Rows by section; a recently used tool shows only under Recent. */
  const sections = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const recents = recent.map((n) => tools.find((t) => t.id === n)).filter((t): t is Tool => !!t && matches(t, q));
    const rest = tools.filter((t) => !recents.includes(t) && matches(t, q));
    return [
      { title: "shell.tools.groups.recent" as Key, items: recents },
      { title: "shell.tools.groups.library" as Key, items: rest.filter((t) => t.group === "library") },
      { title: "shell.tools.groups.stick" as Key, items: rest.filter((t) => t.group === "stick") },
      { title: "shell.tools.groups.app" as Key, items: rest.filter((t) => t.group === "app") },
    ].filter((s) => s.items.length);
  });
  /** Rows in the order the arrows walk them. */
  const shown = $derived(sections.flatMap((s) => s.items));

  $effect(() => {
    void query;
    focus = 0;
  });

  function close() {
    app.toolsOpen = false;
  }

  async function launch(t: Tool) {
    if (t.blocked) return;
    recent = [t.id, ...recent.filter((n) => n !== t.id)].slice(0, 2);
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    close();
    try {
      await t.run();
    } catch (e) {
      app.error = String(e);
    }
  }

  let list = $state<HTMLElement>();
  $effect(() => {
    void focus;
    list?.querySelector(".row.focus")?.scrollIntoView({ block: "nearest" });
  });

  function onKey(e: KeyboardEvent) {
    const n = shown.length;
    const byKey = e.metaKey && tools.find((t) => t.key?.toLowerCase() === e.key.toLowerCase());
    if (byKey) {
      e.preventDefault();
      launch(byKey);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Enter" && shown[focus]) {
      e.preventDefault();
      launch(shown[focus]);
    } else if (n && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      focus = (focus + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    }
  }

  function autofocus(el: HTMLInputElement) {
    el.focus();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="scrim" role="dialog" aria-modal="true" aria-label={tr("shell.tools.title")} tabindex="-1" onclick={(e) => !(e.target as HTMLElement).closest(".palette") && close()} onkeydown={onKey}>
  <div class="palette">
    <label class="search">
      <Search size={20} />
      <input use:autofocus type="text" placeholder={tr("shell.tools.search")} bind:value={query} />
      <kbd>esc</kbd>
    </label>

    <div class="list" bind:this={list}>
      {#each sections as s (s.title)}
        <div class="eyebrow">{tr(s.title)}</div>
        {#each s.items as t (t.id)}
          {@const i = shown.indexOf(t)}
          <button
            class="row"
            class:blocked={!!t.blocked}
            class:focus={i === focus}
            style:--tone={t.tone}
            title={t.blocked || t.hint}
            aria-disabled={!!t.blocked}
            onclick={() => launch(t)}
            onpointermove={() => (focus = i)}
          >
            <span class="icon"><t.icon size={17} strokeWidth={1.8} /></span>
            <span class="text">
              <span class="name">{t.name}</span>
              <span class="hint">{t.blocked || t.hint}</span>
            </span>
            {#if t.key}<span class="keys"><kbd>⌘</kbd><kbd>{t.key}</kbd></span>{/if}
            {#if i === focus && !t.blocked}<CornerDownLeft size={15} class="enter" />{/if}
          </button>
        {/each}
      {/each}
      {#if shown.length === 0}
        <p class="none">{tr("shell.tools.none", { query })}</p>
      {/if}
    </div>

    <footer>
      <span><kbd>↑↓</kbd> {tr("shell.tools.keySelect")}</span>
      <span><kbd>↵</kbd> {tr("shell.tools.keyOpen")}</span>
      <span><kbd>esc</kbd> {tr("shell.tools.keyClose")}</span>
      <span><kbd>⌘K</kbd> {tr("shell.tools.keyAnytime")}</span>
    </footer>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding: 110px 24px 24px;
    background: rgb(5 6 8 / 0.6);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    animation: fade 120ms ease-out;
    outline: none;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .palette {
    width: 680px;
    max-width: 100%;
    max-height: calc(100vh - 160px);
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border: 1px solid var(--hairline);
    border-radius: 18px;
    box-shadow: var(--shadow-pop);
    overflow: hidden;
    animation: drop 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-8px) scale(0.98);
    }
  }
  .search {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 60px;
    padding: 0 20px;
    border-bottom: 1px solid var(--hairline);
    color: var(--text-2);
    flex: none;
  }
  .search input {
    flex: 1;
    height: 40px;
    padding: 0;
    border: 0;
    background: transparent;
    font-size: 17px;
    color: var(--text);
  }
  .search input:focus {
    border: 0;
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 20px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-2);
    font-family: var(--font-ui);
    font-size: 11px;
    font-weight: 500;
    color: var(--text-2);
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 8px 10px;
  }
  .eyebrow {
    padding: 14px 12px 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    height: 52px;
    padding: 0 12px;
    border: 0;
    border-radius: 10px;
    background: none;
    text-align: left;
  }
  .row.focus {
    background: var(--surface-3);
  }
  .icon {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    color: var(--tone);
    background: color-mix(in srgb, var(--tone) 16%, transparent);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .name {
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
  }
  .hint {
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .keys {
    display: flex;
    gap: 4px;
  }
  .row :global(.enter) {
    color: var(--text-2);
    margin-left: 4px;
  }
  .row.blocked {
    cursor: default;
  }
  .row.blocked .icon {
    color: var(--text-3);
    background: var(--surface-2);
  }
  .row.blocked .name {
    color: var(--text-2);
  }
  .row.blocked .hint {
    color: var(--text-3);
  }
  .none {
    margin: 24px 0;
    color: var(--text-2);
    text-align: center;
  }
  footer {
    display: flex;
    gap: 18px;
    padding: 10px 20px;
    border-top: 1px solid var(--hairline);
    font-size: 12px;
    color: var(--text-3);
    flex: none;
  }
  footer span {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
