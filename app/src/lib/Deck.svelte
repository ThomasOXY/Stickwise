<script lang="ts">
  import { Pause, Play } from "@lucide/svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { keyColor, rgb, time } from "./format";
  import { app } from "./state.svelte";
  import { draw } from "./waveform";

  let audio = $state<HTMLAudioElement>();
  let canvas = $state<HTMLCanvasElement>();
  let position = $state(0);
  let playing = $state(false);
  let audioDuration = $state(0);
  let hover = $state<number | null>(null);

  const track = $derived(app.selectedTrack != null ? app.trackById.get(app.selectedTrack) : undefined);
  const detail = $derived(app.detail?.trackId === track?.id ? app.detail : null);
  const duration = $derived(audioDuration || track?.duration || 0);
  const issue = $derived(
    track ? app.verify?.issues.find((i) => i.trackId === track.id && i.severity === "error") : undefined,
  );
  const hotCues = $derived((detail?.cues ?? []).filter((c) => c.slot > 0));
  const memory = $derived((detail?.cues ?? []).filter((c) => c.slot === 0));

  // A newly selected track is loaded paused, like loading a deck.
  $effect(() => {
    void track?.id;
    position = 0;
    playing = false;
    audioDuration = 0;
  });

  function redraw() {
    if (canvas && detail?.preview) draw(canvas, detail.preview);
    else if (canvas) canvas.getContext("2d")?.clearRect(0, 0, canvas.width, canvas.height);
  }

  $effect(() => {
    void detail;
    redraw();
  });

  $effect(() => {
    if (!canvas) return;
    const ro = new ResizeObserver(() => redraw());
    ro.observe(canvas);
    return () => ro.disconnect();
  });

  export function toggle() {
    if (!audio || !track) return;
    if (audio.paused) {
      audio.play().catch((e) => (app.error = `Skladbu nejde přehrát: ${e.message ?? e}`));
    } else {
      audio.pause();
    }
  }

  function seekFromEvent(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    const r = el.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * duration;
  }

  function seek(e: MouseEvent) {
    const t = seekFromEvent(e);
    position = t;
    if (audio) audio.currentTime = t;
  }

  const pct = (s: number) => `${(s / Math.max(1, duration)) * 100}%`;
  const slotLetter = (n: number) => String.fromCharCode(64 + n);
</script>

<footer class="deck">
  {#if track}
    <audio
      bind:this={audio}
      src={convertFileSrc(track.absPath)}
      preload="metadata"
      ontimeupdate={() => (position = audio!.currentTime)}
      onplay={() => (playing = true)}
      onpause={() => (playing = false)}
      onloadedmetadata={() => (audioDuration = audio!.duration || 0)}
      onerror={() => (playing = false)}
    ></audio>

    <div class="info">
      <button class="play" class:on={playing} onclick={toggle} aria-label={playing ? "Pozastavit" : "Přehrát"}>
        {#if playing}<Pause size={15} fill="currentColor" />{:else}<Play size={15} fill="currentColor" />{/if}
      </button>
      <div class="names">
        <div class="t">{track.title}</div>
        <div class="a">{track.artist}</div>
      </div>
      {#if issue}
        <span class="pill"><span class="dot fault"></span>{app.formatOverride.get(track.id) ?? track.format}, na CDJ nepůjde</span>
      {/if}
      <div class="readouts">
        <span class="readout big">{track.bpm.toFixed(2)}</span><span class="unit">BPM</span>
        <i style:background={keyColor(track.key)}></i><span class="readout big">{track.key}</span><span class="unit">Tónina</span>
        <span class="sep"></span>
        <span class="readout mid">{time(position)}</span>
        <span class="readout mid dim">-{time(duration - position)}</span>
      </div>
    </div>

    <div class="wave">
      <div class="flags">
        {#each hotCues as c}
          <span class="flag" style:left={pct(c.timeMs / 1000)} style:background={rgb(c.color, c.slot)}>{slotLetter(c.slot)}</span>
        {/each}
      </div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="canvas-wrap"
        onclick={seek}
        onmousemove={(e) => (hover = seekFromEvent(e))}
        onmouseleave={() => (hover = null)}
      >
        <canvas bind:this={canvas}></canvas>
        {#each hotCues as c}
          <span class="cue-line" style:left={pct(c.timeMs / 1000)} style:background={rgb(c.color, c.slot)}></span>
        {/each}
        <span class="played" style:width={pct(position)}></span>
        <span class="playhead" style:left={pct(position)}></span>
        {#if hover != null}
          <span class="tip readout" style:left={pct(hover)}>{time(hover)}</span>
        {/if}
        {#if !detail?.preview}
          <span class="nowave">{detail ? "Skladba nemá analýzu waveformu" : ""}</span>
        {/if}
      </div>
      <div class="memory">
        {#each memory as c}
          <span class="mem" style:left={pct(c.timeMs / 1000)}></span>
        {/each}
      </div>
    </div>
  {:else}
    <p class="idle">Vyber skladbu v tabulce. Mezerníkem ji pustíš.</p>
  {/if}
</footer>

<style>
  .deck {
    flex: none;
    height: 122px;
    padding: 12px 24px 8px;
    border-top: 1px solid var(--line);
    background: var(--panel);
    display: flex;
    flex-direction: column;
  }
  .info {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .play {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 1px solid var(--line);
    background: var(--panel-2);
    color: var(--text);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .play.on {
    background: var(--cue);
    border-color: var(--cue);
    color: var(--booth);
  }
  .names {
    min-width: 0;
  }
  .t {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .a {
    font-size: 12px;
    color: var(--text-2);
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 10px;
    border-radius: 11px;
    border: 1px solid rgb(229 72 77 / 0.4);
    background: rgb(229 72 77 / 0.1);
    color: #f2a3a5;
    font-size: 12px;
    white-space: nowrap;
  }
  .readouts {
    margin-left: auto;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .big {
    font-size: 28px;
    line-height: 1;
  }
  .mid {
    font-size: 22px;
    line-height: 1;
  }
  .dim {
    color: var(--text-2);
  }
  .unit {
    font-size: 11px;
    color: var(--text-2);
    margin-right: 10px;
  }
  .readouts i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    align-self: center;
  }
  .sep {
    width: 1px;
    height: 22px;
    background: var(--line);
    align-self: center;
    margin: 0 6px;
  }
  .wave {
    position: relative;
    flex: 1;
    margin-top: 8px;
    display: flex;
    flex-direction: column;
  }
  .flags {
    position: relative;
    height: 14px;
  }
  .flag {
    position: absolute;
    top: 0;
    transform: translateX(0);
    width: 12px;
    height: 13px;
    border-radius: 2px;
    font-size: 9px;
    font-weight: 700;
    line-height: 13px;
    text-align: center;
    color: var(--booth);
  }
  .canvas-wrap {
    position: relative;
    flex: 1;
    min-height: 0;
    cursor: pointer;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .cue-line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    opacity: 0.9;
  }
  .played {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: rgb(20 22 27 / 0.45);
    pointer-events: none;
  }
  .playhead {
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
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--booth);
    border: 1px solid var(--line);
    font-size: 13px;
    pointer-events: none;
  }
  .nowave {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
  }
  .memory {
    position: relative;
    height: 8px;
  }
  .mem {
    position: absolute;
    top: 2px;
    margin-left: -4px;
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-bottom: 6px solid var(--text-2);
  }
  .idle {
    margin: auto;
    color: var(--text-2);
  }
</style>
