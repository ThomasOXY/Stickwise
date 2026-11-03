<script lang="ts">
  import { BrainCircuit, Check, LoaderCircle, MapPin, Replace, SquarePlus, WandSparkles, X } from "@lucide/svelte";
  import type { Cue, SmartCue } from "./api";
  import { SLOT_LETTERS, blankCue, firstFreeSlot } from "./cues";
  import { cueTime } from "./format";
  import { t } from "./i18n.svelte";
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
    <span class="tile"><WandSparkles size={16} /></span>
    <div class="titles">
      <span class="title">Smart Cue</span>
      <span class="sub">{t("deck.smart.spots", { count: list.length })} · {t("deck.smart.chosen", { count: chosen.length })}</span>
    </div>
    {#if hasAi}<span class="detr" title={t("deck.smart.detrHint")}><BrainCircuit size={12} /> CUE-DETR</span>{/if}
    <button class="x" onclick={() => (app.smartOpen = false)} aria-label={t("common.close")}><X size={16} /></button>
  </div>
  {#if !hasAi && trackId != null && app.deckHere}
    <div class="ai">
      {#if running}
        <LoaderCircle size={14} class="spin" /> {t("deck.smart.aiRunning")}
      {:else if app.aiInstalled}
        <span>{t("deck.smart.aiPitch")}</span>
        <button class="btn small" onclick={() => app.aiAnalyze([trackId])}><BrainCircuit size={13} /> {t("deck.smart.aiAnalyze")}</button>
      {:else}
        <span>{t("deck.smart.aiMissing")}</span>
      {/if}
    </div>
  {/if}
  <div class="list">
    {#each list as s, i (`${i}:${s.timeMs}`)}
      {@const on = picked.has(s.timeMs)}
      <div class="row" class:on>
        <label class="check">
          <input type="checkbox" checked={on} onchange={() => toggle(s)} aria-label={t("deck.smart.pick", { label: s.label })} />
          <span class="box">{#if on}<Check size={12} strokeWidth={3} />{/if}</span>
        </label>
        <span class="chip" style:background="rgb({s.color[0]} {s.color[1]} {s.color[2]})"></span>
        <button class="time readout" onclick={() => setTime(s.timeMs / 1000)} title={t("deck.smart.jump")}>{cueTime(s.timeMs)}</button>
        <span class="label">{s.label}{#if s.ai}<span class="badge" title={t("deck.smart.aiHint")}>AI</span>{/if}</span>
        <span class="bar">{t("deck.smart.bar", { bar: s.bar })}</span>
        <span class="meter" title={t("deck.smart.strength", { percent: Math.round(s.confidence * 100) })}>
          <span class:high={s.confidence >= 0.7} style:width="{Math.round(s.confidence * 100)}%"></span>
        </span>
      </div>
    {/each}
  </div>
  <div class="foot">
    <button class="btn ghost small" onclick={asMemory} disabled={chosen.length === 0}><MapPin size={14} /> {t("deck.smart.asMemory")}</button>
    <span class="spacer"></span>
    {#if hotCount > 0}
      <button class="btn small" onclick={fillFree} disabled={chosen.length === 0}><SquarePlus size={14} /> {t("deck.smart.freePads")}</button>
      <button class="btn small primary" onclick={replaceHot} disabled={chosen.length === 0}>
        <Replace size={14} /> {t("deck.smart.replace", { n: Math.min(8, chosen.length) })}
      </button>
    {:else}
      <button class="btn small primary" onclick={replaceHot} disabled={chosen.length === 0}>
        {t("deck.smart.toPads", { from: SLOT_LETTERS[0], to: SLOT_LETTERS[Math.min(8, Math.max(1, chosen.length)) - 1] })}
      </button>
    {/if}
  </div>
</div>

<style>
  .panel {
    position: absolute;
    right: 46px;
    bottom: calc(100% - 6px);
    width: 540px;
    background: var(--surface-1);
    border: 1px solid var(--hairline);
    border-radius: 16px;
    box-shadow: var(--shadow-pop);
    z-index: 25;
    display: flex;
    flex-direction: column;
    max-height: 460px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px 12px 18px;
    border-bottom: 1px solid var(--hairline);
  }
  .tile {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .titles {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .detr {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 10px;
    border-radius: 12px;
    background: rgb(155 92 240 / 0.14);
    color: #b48cf5;
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
  }
  .x {
    border: 0;
    background: none;
    color: var(--text-2);
    padding: 4px;
    display: flex;
    border-radius: 6px;
  }
  .x:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .list {
    overflow-y: auto;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 42px;
    padding: 0 10px;
    border-radius: 8px;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row.on {
    background: var(--surface-2);
  }
  .check {
    position: relative;
    display: flex;
    cursor: pointer;
  }
  .check input {
    position: absolute;
    opacity: 0;
    inset: 0;
    margin: 0;
    cursor: pointer;
  }
  .box {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
    color: var(--on-accent);
  }
  .row.on .box {
    background: var(--accent);
    box-shadow: none;
  }
  .check:has(input:focus-visible) .box {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .chip {
    width: 3px;
    height: 20px;
    border-radius: 2px;
    flex: none;
  }
  .time {
    border: 0;
    background: none;
    padding: 0;
    font-size: 16px;
    width: 52px;
    text-align: left;
    flex: none;
  }
  .time:hover {
    color: var(--accent);
  }
  .label {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    font-weight: 500;
  }
  .row:not(.on) .label,
  .row:not(.on) .time {
    color: var(--text-2);
  }
  .badge {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
    box-shadow: inset 0 0 0 1px #9b6cf0;
    color: #b48cf5;
  }
  .bar {
    font-size: 12px;
    color: var(--text-3);
  }
  .meter {
    width: 56px;
    height: 3px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
    flex: none;
  }
  .meter span {
    display: block;
    height: 100%;
    min-width: 4px;
    border-radius: 2px;
    background: var(--text-3);
  }
  .meter span.high {
    background: var(--success);
  }
  .ai {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 10px 16px 2px;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--surface-2);
    font-size: 12px;
    color: var(--text-2);
  }
  .ai span {
    flex: 1;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-top: 1px solid var(--hairline);
  }
  .spacer {
    flex: 1;
  }
  .btn.small {
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
  }
</style>
