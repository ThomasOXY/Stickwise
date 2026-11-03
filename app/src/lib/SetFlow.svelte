<script lang="ts">
  import type { Track } from "./api";
  import { keyColor, keyOrder } from "./format";
  import { app } from "./state.svelte";

  let { tracks }: { tracks: Track[] } = $props();

  const bpms = $derived(tracks.map((t) => t.bpm).filter((b) => b > 0));
  const lo = $derived(bpms.length ? Math.min(...bpms) : 0);
  const hi = $derived(bpms.length ? Math.max(...bpms) : 0);

  /** Bar height in %: the slowest track a third, the fastest full. */
  function height(bpm: number): number {
    if (!bpm) return 14;
    if (hi - lo < 0.5) return 66;
    return 34 + ((bpm - lo) / (hi - lo)) * 66;
  }

  /** Camelot neighbours (same, ±1, A↔B) mix; anything else, or a tempo jump over 6 %, is hard. */
  function hard(a: Track, b: Track): boolean {
    const ka = keyOrder(a.key);
    const kb = keyOrder(b.key);
    if (Number.isFinite(ka) && Number.isFinite(kb)) {
      const [na, la] = [Math.floor(ka / 2), ka % 2];
      const [nb, lb] = [Math.floor(kb / 2), kb % 2];
      const step = Math.min((na - nb + 12) % 12, (nb - na + 12) % 12);
      if (!(step === 0 || (step === 1 && la === lb))) return true;
    }
    return a.bpm > 0 && b.bpm > 0 && Math.abs(a.bpm - b.bpm) / a.bpm > 0.06;
  }

  const hardCount = $derived(tracks.slice(1).filter((t, i) => hard(tracks[i], t)).length);
</script>

<div class="flow">
  <div class="head">
    <span class="label">Průběh setu</span>
    <span class="legend">výška = BPM · barva = tónina</span>
    {#if hardCount > 0}
      <span class="hard"><i></i>{hardCount} {hardCount === 1 ? "tvrdý přechod" : hardCount < 5 ? "tvrdé přechody" : "tvrdých přechodů"}</span>
    {:else if tracks.length > 1}
      <span class="smooth"><i></i>všechny přechody sedí</span>
    {/if}
  </div>
  <div class="bars">
    {#each tracks as t, i (`${i}:${t.id}`)}
      <button
        class="bar"
        class:cur={t.id === app.selectedTrack}
        style:height="{height(t.bpm)}%"
        style:--c={keyColor(t.key)}
        title="{i + 1}. {t.title} · {t.bpm ? t.bpm.toFixed(1) : '–'} BPM · {t.key || '–'}"
        onclick={() => app.selectTrack(t.id)}
        aria-label="{i + 1}. {t.title}"
      ></button>
    {/each}
  </div>
</div>

<style>
  .flow {
    margin: 0 28px 16px;
    padding: 14px 16px 12px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 12px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
    margin-right: auto;
  }
  .hard,
  .smooth {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--warning);
  }
  .smooth {
    color: var(--success);
  }
  .hard i,
  .smooth i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 34px;
  }
  .bar {
    flex: 1;
    min-width: 2px;
    padding: 0;
    border: 0;
    border-radius: 3px 3px 1px 1px;
    background: color-mix(in srgb, var(--c) 62%, var(--surface-2));
    transition: background 0.12s;
  }
  .bar:hover {
    background: var(--c);
  }
  .bar.cur {
    background: var(--c);
    box-shadow: 0 0 0 1.5px var(--text);
  }
</style>
