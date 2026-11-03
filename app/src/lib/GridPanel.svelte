<script lang="ts">
  import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Crosshair, Flag, Grid3x3, Hand, Minus, Plus, RotateCcw, Undo2 } from "@lucide/svelte";
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
  <span class="title" title="Alt + tažení ve waveformu posouvá grid"><Grid3x3 size={15} /> Úprava gridu</span>
  {#if !app.detail?.grid}
    <p class="note">Skladba nemá beatgrid. Nejdřív ji analyzuj.</p>
  {:else if !editable}
    <p class="note">Beatgrid se upravuje v knihovně (Tento Mac). Na flashku se dostane synchronizací.</p>
  {:else if grid}
    <div class="pill">
      <button class="g icon" onclick={(e) => tempo(grid.bpm - (e.shiftKey ? 0.1 : 0.01))} title="−0,01 BPM (se Shiftem −0,1)" aria-label="Pomalejší">
        <Minus size={14} />
      </button>
      <label class="bpm">
        <input
          class="readout"
          bind:value={bpmText}
          onchange={typed}
          onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
          aria-label="Tempo gridu"
        />
        <span>BPM</span>
      </label>
      <button class="g icon" onclick={(e) => tempo(grid.bpm + (e.shiftKey ? 0.1 : 0.01))} title="+0,01 BPM (se Shiftem +0,1)" aria-label="Rychlejší">
        <Plus size={14} />
      </button>
    </div>

    <div class="pill">
      <button class="g" onclick={() => tempo(grid.bpm / 2)} title="Poloviční tempo">÷2</button>
      <button class="g" onclick={() => tempo(grid.bpm * 2)} title="Dvojnásobné tempo">×2</button>
      <button class="g" onclick={tap} title="Ťukej do rytmu (aspoň 4×)">
        <Hand size={13} /> TAP{#if tapCount > 0 && tapCount < 4}<span class="n">{tapCount}</span>{/if}
      </button>
    </div>

    <div class="pill">
      <button class="g icon" onclick={() => shift(-10)} title="Grid o 10 ms dřív" aria-label="Grid o 10 ms dřív"><ChevronsLeft size={14} /></button>
      <button class="g icon" onclick={() => shift(-1)} title="Grid o 1 ms dřív" aria-label="Grid o 1 ms dřív"><ChevronLeft size={14} /></button>
      <span class="lbl">posun</span>
      <button class="g icon" onclick={() => shift(1)} title="Grid o 1 ms později" aria-label="Grid o 1 ms později"><ChevronRight size={14} /></button>
      <button class="g icon" onclick={() => shift(10)} title="Grid o 10 ms později" aria-label="Grid o 10 ms později"><ChevronsRight size={14} /></button>
    </div>

    <div class="pill">
      <button class="g icon" onclick={beatHere} title="Doba sem: nejbližší doba přesně na playhead" aria-label="Doba sem"><Crosshair size={14} /></button>
      <button class="g" onclick={downbeatHere} title="Doba na playheadu bude první doba taktu"><Flag size={13} /> Tady je 1. doba</button>
      <button class="g icon" onclick={() => moveBar(-1)} title="První doba taktu o dobu dřív" aria-label="Takt o dobu dřív"><ChevronLeft size={14} /></button>
      <button class="g icon" onclick={() => moveBar(1)} title="První doba taktu o dobu později" aria-label="Takt o dobu později"><ChevronRight size={14} /></button>
    </div>

    <span class="spacer"></span>

    <button
      class="switch"
      class:on={app.metronome}
      role="switch"
      aria-checked={app.metronome}
      onclick={() => (app.metronome = !app.metronome)}
      title="Klik na každou dobu při přehrávání"
    >
      <span class="knob"></span> Metronom
    </button>
    {#if grid.edited}
      <span class="badge" title="Grid se liší od analýzy">upraveno</span>
      <button class="g text" onclick={() => app.resetGrid()} title="Zahodit úpravy a vrátit grid z analýzy"><RotateCcw size={13} /> Vrátit analýzu</button>
    {/if}
    <button class="g icon" disabled={!app.canUndoGrid} onclick={() => app.undoGrid()} title="Vrátit poslední úpravu gridu (⌘Z)" aria-label="Vrátit úpravu gridu">
      <Undo2 size={14} />
    </button>
  {/if}
</div>

<style>
  .grid-panel {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 52px;
    padding: 8px 10px 8px 14px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 7%, var(--surface-1));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 22%, transparent);
    min-width: 0;
    overflow-x: auto;
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-right: 8px;
    color: var(--accent);
    font-size: 13.5px;
    font-weight: 600;
    white-space: nowrap;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
    height: 36px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--surface-1);
  }
  .g {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
    flex: none;
  }
  .g :global(svg) {
    color: var(--text-2);
  }
  .g:hover {
    background: var(--surface-3);
  }
  .g:disabled {
    opacity: 0.4;
    pointer-events: none;
  }
  .g.icon {
    width: 28px;
    padding: 0;
  }
  .g.text {
    color: var(--text);
  }
  .lbl {
    font-size: 12px;
    color: var(--text-3);
    padding: 0 4px;
  }
  .bpm {
    display: flex;
    align-items: baseline;
    gap: 4px;
    height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    background: var(--surface-3);
  }
  .bpm input {
    width: 54px;
    height: 28px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font-size: 18px;
    text-align: right;
    outline: none;
  }
  .bpm span {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-3);
  }
  .n {
    font-size: 10px;
    color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 6px;
    border: 0;
    background: none;
    color: var(--text-2);
    font-size: 12.5px;
    white-space: nowrap;
    flex: none;
  }
  .knob {
    position: relative;
    width: 34px;
    height: 20px;
    border-radius: 10px;
    background: var(--surface-3);
    transition: background 0.15s;
  }
  .knob::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-2);
    transition: transform 0.15s;
  }
  .switch.on .knob {
    background: var(--accent);
  }
  .switch.on .knob::after {
    transform: translateX(14px);
    background: var(--on-accent);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 9px;
    border-radius: 11px;
    background: var(--surface-1);
    color: var(--text-2);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .badge::before {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .note {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-2);
  }
</style>
