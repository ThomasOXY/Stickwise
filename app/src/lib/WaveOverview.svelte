<script lang="ts">
  import type { Cue, Waveform } from "./api";
  import { time } from "./format";
  import { DETAIL_RATE, columnCount, paint } from "./waveform";

  interface Props {
    /** Whole-track waveform, or a detail one (150 columns/s) for styles without a preview. */
    wave: Waveform | null;
    isDetail: boolean;
    cues: Cue[];
    ghosts: { timeMs: number; label: string; color: string }[];
    duration: number;
    position: number;
    span: number;
    onSeek: (t: number) => void;
  }

  let { wave, isDetail, cues, ghosts, duration, position, span, onSeek }: Props = $props();

  let canvas = $state<HTMLCanvasElement>();
  let hover = $state<number | null>(null);
  let seeking = false;

  const HOT = ["#ee5fb0", "#e8423c", "#f28a2e", "#e6d23f", "#4cc768", "#3fcfd4", "#3e7bf2", "#9b5cf0"];
  const colorOf = (c: Cue) =>
    c.color && (c.color[0] || c.color[1] || c.color[2]) ? `rgb(${c.color[0]} ${c.color[1]} ${c.color[2]})` : HOT[c.slot - 1];

  function repaint() {
    if (!canvas || !wave || duration <= 0) return;
    const rate = isDetail ? DETAIL_RATE : columnCount(wave) / duration;
    paint(canvas, wave, rate, 0, duration);
  }

  $effect(() => {
    void [wave, duration, isDetail];
    repaint();
  });

  $effect(() => {
    if (!canvas) return;
    const ro = new ResizeObserver(repaint);
    ro.observe(canvas);
    return () => ro.disconnect();
  });

  const pct = (s: number) => `${(s / Math.max(1, duration)) * 100}%`;
  const at = (e: PointerEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * duration;
  };
</script>

<div class="overview">
  <div class="flags">
    {#each cues.filter((c) => c.slot > 0) as c}
      <span class="flag" style:left={pct(c.timeMs / 1000)} style:background={colorOf(c)}>{String.fromCharCode(64 + c.slot)}</span>
    {/each}
  </div>
  <div
    class="track"
    role="slider"
    tabindex="-1"
    aria-label="Pozice ve skladbě"
    aria-valuemin={0}
    aria-valuemax={Math.round(duration)}
    aria-valuenow={Math.round(position)}
    onpointerdown={(e) => {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      seeking = true;
      onSeek(at(e));
    }}
    onpointermove={(e) => {
      hover = at(e);
      if (seeking) onSeek(at(e));
    }}
    onpointerup={() => (seeking = false)}
    onpointerleave={() => (hover = null)}
  >
    <canvas bind:this={canvas}></canvas>
    <span class="played" style:width={pct(position)}></span>
    {#each ghosts as g}
      <span class="ghost" style:left={pct(g.timeMs / 1000)} style:border-color={g.color} title={g.label}></span>
    {/each}
    <span class="window" style:left={pct(position - span / 2)} style:width={pct(span)}></span>
    <span class="head" style:left={pct(position)}></span>
    {#if hover != null}
      <span class="tip readout" style:left={pct(hover)}>{time(hover)}</span>
    {/if}
    {#if !wave}<span class="none">Skladba nemá analýzu waveformu</span>{/if}
  </div>
  <div class="mems">
    {#each cues.filter((c) => c.slot === 0) as c}
      <span class="mem" style:left={pct(c.timeMs / 1000)}></span>
    {/each}
  </div>
</div>

<style>
  .overview {
    display: flex;
    flex-direction: column;
  }
  .flags {
    position: relative;
    height: 13px;
  }
  .flag {
    position: absolute;
    width: 11px;
    height: 12px;
    border-radius: 2px;
    font-size: 8.5px;
    font-weight: 700;
    line-height: 12px;
    text-align: center;
    color: var(--booth);
  }
  .track {
    position: relative;
    height: 34px;
    cursor: pointer;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .played {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: rgb(20 22 27 / 0.5);
    pointer-events: none;
  }
  .ghost {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 1px dashed;
    pointer-events: none;
  }
  .window {
    position: absolute;
    top: -1px;
    bottom: -1px;
    border: 1px solid rgb(255 255 255 / 0.55);
    border-radius: 2px;
    pointer-events: none;
  }
  .head {
    position: absolute;
    top: -2px;
    bottom: -2px;
    width: 2px;
    margin-left: -1px;
    background: var(--cue);
    pointer-events: none;
  }
  .tip {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    padding: 0 6px;
    border-radius: 4px;
    background: var(--booth);
    border: 1px solid var(--line);
    font-size: 13px;
    pointer-events: none;
  }
  .none {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
  }
  .mems {
    position: relative;
    height: 7px;
  }
  .mem {
    position: absolute;
    top: 1px;
    margin-left: -4px;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-bottom: 6px solid var(--fault);
  }
</style>
