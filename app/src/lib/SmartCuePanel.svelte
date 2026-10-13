<script lang="ts">
  import { BrainCircuit, LoaderCircle, Sparkles, X } from "@lucide/svelte";
  import type { Cue, SmartCue } from "./api";
  import { SLOT_LETTERS, blankCue, firstFreeSlot } from "./cues";
  import { cueTime } from "./format";
  import { app } from "./state.svelte";

  interface Props {
    setTime: (t: number) => void;
  }
  let { setTime }: Props = $props();

  const list = $derived(app.smart ?? []);
  let picked = $state<Set<number>>(new Set());
  $effect(() => {
    picked = new Set(list.filter((s) => s.inPlan).map((s) => s.timeMs));
  });

  const cues = $derived(app.detail?.cues ?? []);
  const hotCount = $derived(cues.filter((c) => c.slot > 0).length);
  const chosen = $derived(list.filter((s) => picked.has(s.timeMs)));
  const trackId = $derived(app.detail?.trackId);
  const hasAi = $derived(list.some((s) => s.ai));
  const running = $derived(app.aiJob != null);

  function toggle(s: SmartCue) {
    const next = new Set(picked);
    next.has(s.timeMs) ? next.delete(s.timeMs) : next.add(s.timeMs);
    picked = next;
  }

  function asHot(s: SmartCue, slot: number): Cue {
    return { ...blankCue(slot, s.timeMs), hotColor: s.colorCode, comment: s.label };
  }

  /** Replace all hot cues with the chosen suggestions, A..H in time order. */
  function replaceHot() {
    const keep = cues.filter((c) => c.slot === 0);
    app.setCues([...keep, ...chosen.slice(0, 8).map((s, i) => asHot(s, i + 1))]);
    app.smartOpen = false;
  }

  /** Put the chosen suggestions on free pads only. */
  function fillFree() {
    let next = [...cues];
    for (const s of chosen) {
      if (next.some((c) => c.slot > 0 && Math.abs(c.timeMs - s.timeMs) < 50)) continue;
      const slot = firstFreeSlot(next);
      if (slot == null) break;
      next = [...next, asHot(s, slot)];
    }
    app.setCues(next);
    app.smartOpen = false;
  }

  function asMemory() {
    const next = [...cues];
    for (const s of chosen) {
      if (!next.some((c) => c.slot === 0 && Math.abs(c.timeMs - s.timeMs) < 50)) {
        next.push({ ...blankCue(0, s.timeMs), comment: s.label });
      }
    }
    app.setCues(next);
    app.smartOpen = false;
  }
</script>

<div class="panel" role="dialog" aria-label="Smart Cue">
  <div class="head">
    <Sparkles size={14} />
    <span class="title">Smart Cue</span>
    <span class="sub">{list.length} míst, {chosen.length} vybráno</span>
    <button class="x" onclick={() => (app.smartOpen = false)} aria-label="Zavřít"><X size={14} /></button>
  </div>
  {#if !hasAi && trackId != null}
    <div class="ai">
      {#if running}
        <LoaderCircle size={14} class="spin" /> AI analýza běží…
      {:else if app.aiInstalled}
        <span>Přesnější body z modelu natrénovaného na cue profi DJů.</span>
        <button class="btn small" onclick={() => app.aiAnalyze([trackId])}><BrainCircuit size={13} /> AI analýza</button>
      {:else}
        <span class="sub">AI analýza (CUE-DETR) není nainstalovaná. V terminálu: cd sidecar && uv sync</span>
      {/if}
    </div>
  {/if}
  <div class="list">
    {#each list as s, i (`${i}:${s.timeMs}`)}
      <div class="row" class:off={!picked.has(s.timeMs)}>
        <input type="checkbox" checked={picked.has(s.timeMs)} onchange={() => toggle(s)} aria-label="Vybrat {s.label}" />
        <span class="chip" style:background="rgb({s.color[0]} {s.color[1]} {s.color[2]})"></span>
        <button class="time readout" onclick={() => setTime(s.timeMs / 1000)} title="Skočit sem">{cueTime(s.timeMs)}</button>
        <span class="label">{s.label}</span>
        {#if s.ai}<span class="badge" title="Bod navrhl CUE-DETR">AI</span>{/if}
        <span class="bar">takt {s.bar}</span>
        <span class="conf" style:width="{Math.round(s.confidence * 28)}px" title="Jak výrazná je změna"></span>
      </div>
    {/each}
  </div>
  <div class="foot">
    {#if hotCount > 0}
      <button class="btn small" onclick={fillFree} disabled={chosen.length === 0}>Doplnit volné pady</button>
      <button class="btn small primary" onclick={replaceHot} disabled={chosen.length === 0}>
        Nahradit hot cue ({Math.min(8, chosen.length)})
      </button>
    {:else}
      <button class="btn small primary" onclick={replaceHot} disabled={chosen.length === 0}>
        Na pady {SLOT_LETTERS[0]}–{SLOT_LETTERS[Math.min(8, Math.max(1, chosen.length)) - 1]}
      </button>
    {/if}
    <button class="btn small" onclick={asMemory} disabled={chosen.length === 0}>Jako memory cue</button>
  </div>
</div>

<style>
  .panel {
    position: absolute;
    right: 24px;
    bottom: calc(100% + 8px);
    width: 380px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
    z-index: 25;
    display: flex;
    flex-direction: column;
    max-height: 420px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px 8px;
    color: var(--cue);
  }
  .title {
    font-weight: 600;
    color: var(--text);
  }
  .sub {
    font-size: 12px;
    color: var(--text-2);
  }
  .x {
    margin-left: auto;
    border: 0;
    background: none;
    color: var(--text-2);
    padding: 2px;
  }
  .list {
    overflow-y: auto;
    padding: 0 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border-radius: 4px;
  }
  .row:hover {
    background: var(--panel-2);
  }
  .row.off {
    opacity: 0.5;
  }
  .row input {
    accent-color: var(--cue);
    margin: 0;
  }
  .chip {
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .time {
    border: 0;
    background: none;
    padding: 0;
    font-size: 15px;
    width: 56px;
    text-align: left;
  }
  .time:hover {
    color: var(--cue);
  }
  .label {
    flex: 1;
    font-weight: 500;
  }
  .ai {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 14px 8px;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--booth);
    font-size: 12px;
    color: var(--text-2);
  }
  .ai span {
    flex: 1;
  }
  .badge {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 3px;
    border: 1px solid var(--cue);
    color: var(--cue);
  }
  .bar {
    font-size: 11px;
    color: var(--text-2);
  }
  .conf {
    height: 3px;
    border-radius: 2px;
    background: var(--text-3);
    min-width: 4px;
  }
  .foot {
    display: flex;
    gap: 6px;
    padding: 10px 14px 12px;
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
</style>
