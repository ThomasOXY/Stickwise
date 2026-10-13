<script lang="ts">
  import { Minus, Pause, Play, Plus, Sparkles } from "@lucide/svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { keyColor, time } from "./format";
  import { app } from "./state.svelte";
  import CuePads from "./CuePads.svelte";
  import SmartCuePanel from "./SmartCuePanel.svelte";
  import WaveDetail from "./WaveDetail.svelte";
  import WaveOverview from "./WaveOverview.svelte";
  import { STYLE_LABELS, type Style } from "./waveform";

  const MIN_SPAN = 2;
  const MAX_SPAN = 64;

  let audio = $state<HTMLAudioElement>();
  let position = $state(0);
  let playing = $state(false);
  let audioDuration = $state(0);
  let style = $state<Style>((localStorage.getItem("wave-style") as Style) || "three-band");
  let span = $state(Number(localStorage.getItem("wave-span")) || 8);
  let resumeAfterScrub = false;

  const track = $derived(app.selectedTrack != null ? app.trackById.get(app.selectedTrack) : undefined);
  const detail = $derived(app.detail?.trackId === track?.id ? app.detail : null);
  const duration = $derived(audioDuration || track?.duration || 0);
  const issue = $derived(
    track ? app.verify?.issues.find((i) => i.trackId === track.id && i.severity === "error") : undefined,
  );
  const cues = $derived(detail?.cues ?? []);
  const ghosts = $derived(
    app.smartOpen && app.smart
      ? app.smart.map((s) => ({ timeMs: s.timeMs, label: s.label, color: `rgb(${s.color[0]} ${s.color[1]} ${s.color[2]})` }))
      : [],
  );

  const styles = $derived((["three-band", "color", "mono"] as Style[]).filter((s) => detail?.details.some((w) => w.kind === s)));
  const activeStyle = $derived(styles.includes(style) ? style : (styles[0] ?? style));
  const detailWave = $derived(detail?.details.find((w) => w.kind === activeStyle) ?? null);
  const overview = $derived.by(() => {
    const p = detail?.previews.find((w) => w.kind === activeStyle);
    if (p) return { wave: p, isDetail: false };
    return { wave: detailWave, isDetail: true };
  });

  $effect(() => localStorage.setItem("wave-style", style));
  $effect(() => localStorage.setItem("wave-span", String(span)));

  // A newly selected track is loaded paused, like loading a deck.
  $effect(() => {
    void track?.id;
    position = 0;
    playing = false;
    audioDuration = 0;
  });

  // Follow the audio clock smoothly while playing.
  $effect(() => {
    if (!playing) return;
    let raf = 0;
    const tick = () => {
      if (audio) position = audio.currentTime;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  const getTime = () => (playing && audio ? audio.currentTime : position);

  function setTime(t: number) {
    position = t;
    if (audio) audio.currentTime = t;
  }

  export function toggle() {
    if (!audio || !track) return;
    if (audio.paused) audio.play().catch((e) => (app.error = `Skladbu nejde přehrát: ${e.message ?? e}`));
    else audio.pause();
  }

  export function zoom(factor: number) {
    span = Math.min(MAX_SPAN, Math.max(MIN_SPAN, span * factor));
  }

  function scrubStart() {
    resumeAfterScrub = playing;
    if (playing) audio?.pause();
  }

  function scrubEnd() {
    if (resumeAfterScrub) audio?.play();
    resumeAfterScrub = false;
  }
</script>

<footer class="deck">
  {#if track}
    <audio
      bind:this={audio}
      src={convertFileSrc(track.absPath)}
      preload="auto"
      ontimeupdate={() => !playing && (position = audio!.currentTime)}
      onplay={() => (playing = true)}
      onpause={() => (playing = false)}
      onended={() => (playing = false)}
      onloadedmetadata={() => (audioDuration = audio!.duration || 0)}
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
      {#if detail?.edited}
        <span class="edited">Cue upravené, zatím jen v aplikaci <button onclick={() => app.revertCues()}>Vrátit</button></span>
      {/if}

      <div class="tools">
        <button
          class="btn small smart"
          class:on={app.smartOpen}
          onclick={() => (app.smartOpen ? (app.smartOpen = false) : app.loadSmart())}
          title="Smart Cue: najde dropy, breaky a další místa pro cue"
        >
          <Sparkles size={13} /> Smart Cue
        </button>
        {#if styles.length > 1}
          <div class="seg" role="radiogroup" aria-label="Styl waveformu">
            {#each styles as s}
              <button role="radio" aria-checked={activeStyle === s} class:on={activeStyle === s} onclick={() => (style = s)}>
                {STYLE_LABELS[s]}
              </button>
            {/each}
          </div>
        {/if}
        <div class="zoom">
          <button class="btn icon" onclick={() => zoom(1.5)} aria-label="Oddálit" disabled={span >= MAX_SPAN}><Minus size={13} /></button>
          <span class="readout zv">{span < 10 ? span.toFixed(1) : Math.round(span)} s</span>
          <button class="btn icon" onclick={() => zoom(1 / 1.5)} aria-label="Přiblížit" disabled={span <= MIN_SPAN}><Plus size={13} /></button>
        </div>
      </div>

      <div class="readouts">
        <span class="readout big">{track.bpm.toFixed(2)}</span><span class="unit">BPM</span>
        <i style:background={keyColor(track.key)}></i><span class="readout big">{track.key}</span><span class="unit">Tónina</span>
        <span class="sep"></span>
        <span class="readout mid">{time(position)}</span>
        <span class="readout mid dim">-{time(duration - position)}</span>
      </div>
    </div>

    <div class="detail">
      <WaveDetail
        wave={detailWave}
        beats={detail?.beats ?? []}
        {cues}
        {ghosts}
        {duration}
        {span}
        {playing}
        {getTime}
        {setTime}
        onZoom={zoom}
        onScrubStart={scrubStart}
        onScrubEnd={scrubEnd}
      />
    </div>

    <WaveOverview wave={overview.wave} isDetail={overview.isDetail} {cues} {ghosts} {duration} {position} {span} onSeek={setTime} />

    {#if detail}
      <CuePads {getTime} {setTime} beats={detail.beats} bpm={track.bpm} />
    {/if}
    {#if app.smartOpen && app.smart}
      <SmartCuePanel {setTime} />
    {/if}
  {:else}
    <p class="idle">Vyber skladbu v tabulce. Mezerníkem ji pustíš, kolečkem myši nad waveformem přiblížíš.</p>
  {/if}
</footer>

<style>
  .deck {
    position: relative;
    flex: none;
    height: 278px;
    padding: 12px 24px 6px;
    border-top: 1px solid var(--line);
    background: var(--panel);
    display: flex;
    flex-direction: column;
    gap: 8px;
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
    max-width: 360px;
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
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .tools {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-left: auto;
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
  .btn.smart.on {
    border-color: var(--cue);
    color: var(--cue);
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 2px;
    background: var(--booth);
  }
  .seg button {
    border: 0;
    background: none;
    height: 22px;
    padding: 0 9px;
    border-radius: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .seg button.on {
    background: var(--panel-2);
    color: var(--text);
  }
  .zoom {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .zoom .btn.icon {
    width: 24px;
    height: 24px;
  }
  .zv {
    min-width: 38px;
    text-align: center;
    font-size: 15px;
  }
  .readouts {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-left: 14px;
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
  .edited {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--cue);
    white-space: nowrap;
  }
  .edited button {
    border: 0;
    background: none;
    padding: 0;
    color: var(--text-2);
    text-decoration: underline;
    font-size: 12px;
  }
  .detail {
    flex: 1;
    min-height: 0;
  }
  .idle {
    margin: auto;
    color: var(--text-2);
  }
</style>
