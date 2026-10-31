<script lang="ts">
  import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Crosshair, Flag, Hand, Metronome, RotateCcw, Undo2 } from "@lucide/svelte";
  import { nearestBeat, periodMs, withTempo, type Grid } from "./grid";
  import { app } from "./state.svelte";

  interface Props {
    /** Playhead, seconds. */
    getTime: () => number;
  }

  let { getTime }: Props = $props();

  const grid = $derived(app.detail?.grid ?? null);
  const editable = $derived(app.gridEditable);
  let bpmText = $state("");
  let taps: number[] = [];
  let tapCount = $state(0);

  $effect(() => {
    if (grid) bpmText = grid.bpm.toFixed(2);
  });

  const here = () => getTime() * 1000;

  function set(next: Grid) {
    app.setGrid(next);
  }

  /** Move the whole grid by `ms`. */
  function shift(ms: number) {
    if (grid) set({ bpm: grid.bpm, anchorMs: grid.anchorMs + ms });
  }

  /** The tempo changes; the beat at the playhead stays put. */
  function tempo(bpm: number) {
    if (grid) set(withTempo(grid, Math.round(bpm * 1000) / 1000, here()));
  }

  /** The nearest beat lands exactly on the playhead; bars stay as they are. */
  function beatHere() {
    if (!grid) return;
    const k = nearestBeat(grid, here());
    shift(here() - (grid.anchorMs + k * periodMs(grid)));
  }

  /** The beat at the playhead becomes beat 1 of a bar. */
  function downbeatHere() {
    if (grid) set({ bpm: grid.bpm, anchorMs: here() });
  }

  /** Which beat is the bar one: one beat earlier or later. */
  function moveBar(dir: -1 | 1) {
    if (grid) set({ bpm: grid.bpm, anchorMs: grid.anchorMs + dir * periodMs(grid) });
  }

  function typed() {
    const v = Number(bpmText.replace(",", "."));
    if (grid && v >= 20 && v <= 400) tempo(v);
    else if (grid) bpmText = grid.bpm.toFixed(2);
  }

  /** Tap along with the beat: the tempo is the average over the taps. */
  function tap() {
    const now = performance.now();
    if (taps.length && now - taps[taps.length - 1] > 2000) taps = [];
    taps.push(now);
    tapCount = taps.length;
    if (taps.length >= 4) {
      const span = taps[taps.length - 1] - taps[0];
      tempo((60_000 * (taps.length - 1)) / span);
    }
  }

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.tagName === "INPUT" || app.toolsOpen) return;
    if (e.key === "z" && e.metaKey) {
      e.preventDefault();
      app.undoGrid();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="grid-panel">
  {#if !app.detail?.grid}
    <p class="note">Skladba nemá beatgrid. Nejdřív ji analyzuj.</p>
  {:else if !editable}
    <p class="note">Beatgrid se upravuje v knihovně (Tento Mac). Na flashku se dostane synchronizací.</p>
  {:else if grid}
    <div class="group">
      <span class="lbl">BPM</span>
      <button class="btn small" onclick={() => tempo(grid.bpm - 0.1)} title="−0,1 BPM">−.1</button>
      <button class="btn small" onclick={() => tempo(grid.bpm - 0.01)} title="−0,01 BPM">−.01</button>
      <input
        class="bpm readout"
        bind:value={bpmText}
        onchange={typed}
        onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
        aria-label="Tempo gridu"
      />
      <button class="btn small" onclick={() => tempo(grid.bpm + 0.01)} title="+0,01 BPM">+.01</button>
      <button class="btn small" onclick={() => tempo(grid.bpm + 0.1)} title="+0,1 BPM">+.1</button>
      <button class="btn small" onclick={() => tempo(grid.bpm / 2)} title="Poloviční tempo">÷2</button>
      <button class="btn small" onclick={() => tempo(grid.bpm * 2)} title="Dvojnásobné tempo">×2</button>
      <button class="btn small" onclick={tap} title="Ťukej do rytmu (aspoň 4×)">
        <Hand size={12} /> TAP{#if tapCount > 0 && tapCount < 4}<span class="n">{tapCount}</span>{/if}
      </button>
    </div>

    <div class="group">
      <span class="lbl">Posun</span>
      <button class="btn small icon" onclick={() => shift(-10)} title="Grid o 10 ms dřív"><ChevronsLeft size={13} /></button>
      <button class="btn small icon" onclick={() => shift(-1)} title="Grid o 1 ms dřív"><ChevronLeft size={13} /></button>
      <button class="btn small icon" onclick={() => shift(1)} title="Grid o 1 ms později"><ChevronRight size={13} /></button>
      <button class="btn small icon" onclick={() => shift(10)} title="Grid o 10 ms později"><ChevronsRight size={13} /></button>
    </div>

    <div class="group">
      <button class="btn small" onclick={beatHere} title="Nejbližší doba přesně na playhead"><Crosshair size={12} /> Doba sem</button>
      <button class="btn small" onclick={downbeatHere} title="Doba na playheadu bude první doba taktu"><Flag size={12} /> 1. doba sem</button>
      <button class="btn small icon" onclick={() => moveBar(-1)} title="První doba taktu o dobu dřív"><ChevronLeft size={13} /></button>
      <span class="lbl bar">Takt</span>
      <button class="btn small icon" onclick={() => moveBar(1)} title="První doba taktu o dobu později"><ChevronRight size={13} /></button>
    </div>

    <div class="group end">
      <button class="btn small" class:on={app.metronome} onclick={() => (app.metronome = !app.metronome)} title="Klik na každou dobu při přehrávání">
        <Metronome size={12} /> Metronom
      </button>
      <button class="btn small icon" disabled={!app.canUndoGrid} onclick={() => app.undoGrid()} title="Vrátit poslední úpravu gridu (⌘Z)"><Undo2 size={13} /></button>
      {#if grid.edited}
        <button class="btn small" onclick={() => app.resetGrid()} title="Zahodit úpravy a vrátit grid z analýzy"><RotateCcw size={12} /> Z analýzy</button>
      {/if}
    </div>
    <span class="hint">Alt + tažení ve waveformu posouvá grid</span>
  {/if}
</div>

<style>
  .grid-panel {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 16px;
    min-height: 30px;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .group.end {
    margin-left: auto;
  }
  .lbl {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
    margin-right: 2px;
  }
  .lbl.bar {
    margin: 0 2px;
  }
  .btn.small {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
    gap: 4px;
  }
  .btn.small.icon {
    width: 26px;
    padding: 0;
    justify-content: center;
  }
  .btn.on {
    border-color: var(--cue);
    color: var(--cue);
  }
  .bpm {
    width: 74px;
    height: 26px;
    text-align: center;
    font-size: 17px;
    padding: 0 4px;
  }
  .n {
    font-size: 10px;
    color: var(--cue);
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
