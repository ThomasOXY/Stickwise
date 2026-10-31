<script lang="ts">
  import { Grid3x3, Minus, Pause, Play, Plus, Sparkles, Volume1, Volume2, VolumeX } from "@lucide/svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { keyColor, time } from "./format";
  import { app } from "./state.svelte";
  import CuePads from "./CuePads.svelte";
  import GridPanel from "./GridPanel.svelte";
  import type { Grid } from "./grid";
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
  /** Volume fader 0..1; the gain follows its square, which sounds even across the travel. */
  let fader = $state(Number(localStorage.getItem("volume") ?? 1));
  let muted = $state(localStorage.getItem("muted") === "on");
  let resumeAfterScrub = false;

  // The deck keeps its track while the app switches between library and stick.
  const track = $derived(app.deck?.track);
  const detail = $derived(app.detail?.trackId === track?.id ? app.detail : null);
  const duration = $derived(audioDuration || track?.duration || 0);
  const issue = $derived(
    track && app.deckHere ? app.verify?.issues.find((i) => i.trackId === track.id && i.severity === "error") : undefined,
  );
  const cues = $derived(detail?.cues ?? []);
  const from = $derived(
    app.deck?.source === "library"
      ? "Hraje z knihovny"
      : `Hraje z flashky ${app.sticks.find((s) => s.path === app.deck?.stickPath)?.name ?? ""}`,
  );
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
  $effect(() => localStorage.setItem("volume", String(fader)));
  $effect(() => localStorage.setItem("muted", muted ? "on" : "off"));
  $effect(() => {
    if (!audio) return;
    audio.volume = fader * fader;
    audio.muted = muted;
  });

  function setFader(v: number) {
    fader = Math.min(1, Math.max(0, v));
    muted = fader === 0;
  }

  function onVolumeWheel(e: WheelEvent) {
    e.preventDefault();
    setFader((muted ? 0 : fader) - Math.sign(e.deltaY || -e.deltaX) * 0.05);
  }

  // A newly selected track is loaded paused, like loading a deck.
  $effect(() => {
    void app.deck;
    position = 0;
    playing = false;
    audioDuration = 0;
  });

  // WebKit moves `currentTime` in coarse steps: run our own clock from the
  // frame time and only pull it gently towards the audio, so the waveform
  // scrolls evenly instead of jerking.
  let clock = { media: 0, at: 0 };
  let last = { at: -1, t: 0 };

  function resync() {
    clock = { media: audio?.currentTime ?? position, at: performance.now() };
    last.at = -1;
  }

  function smoothTime(): number {
    if (!audio) return position;
    const now = performance.now();
    if (now - last.at < 4) return last.t;
    const est = clock.media + ((now - clock.at) / 1000) * audio.playbackRate;
    const real = audio.currentTime;
    clock = Math.abs(real - est) > 0.15 ? { media: real, at: now } : { media: est + (real - est) * 0.05, at: now };
    last = { at: now, t: clock.media };
    return clock.media;
  }

  // Follow the audio clock smoothly while playing.
  $effect(() => {
    if (!playing) return;
    resync();
    let raf = 0;
    const tick = () => {
      position = smoothTime();
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  const getTime = () => (playing && audio ? smoothTime() : position);

  function setTime(t: number) {
    position = t;
    if (audio) audio.currentTime = t;
    resync();
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

  // Alt + drag in the waveform moves the grid (in grid mode).
  let dragBase: Grid | null = null;
  let dragMoved = false;
  function gridDrag(deltaMs: number, phase: "start" | "move" | "end") {
    const g = app.detail?.grid;
    if (!g || !app.gridEditable) return;
    if (phase === "start") {
      dragBase = { bpm: g.bpm, anchorMs: g.anchorMs };
      dragMoved = false;
      return;
    }
    if (!dragBase) return;
    if (deltaMs !== 0) {
      // One undo step for the whole drag.
      app.setGrid({ bpm: dragBase.bpm, anchorMs: dragBase.anchorMs + deltaMs }, !dragMoved);
      dragMoved = true;
    }
    if (phase === "end") dragBase = null;
  }

  // Metronome: a click on every beat (higher on bar ones), scheduled a
  // little ahead on the Web Audio clock.
  let clickCtx: AudioContext | null = null;
  function click(delay: number, accent: boolean) {
    const ctx = clickCtx!;
    const t = ctx.currentTime + delay;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.frequency.value = accent ? 1760 : 1175;
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(0.5, t + 0.002);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.05);
    osc.connect(gain).connect(ctx.destination);
    osc.start(t);
    osc.stop(t + 0.06);
  }
  $effect(() => {
    if (!playing || !app.metronome) return;
    clickCtx ??= new AudioContext();
    void clickCtx.resume();
    let done = getTime() * 1000;
    let raf = 0;
    const loop = () => {
      const now = getTime() * 1000;
      // After a jump, start clicking from where the music is.
      if (now < done - 250 || now > done + 1000) done = now;
      const rate = audio?.playbackRate || 1;
      for (const [ms, n] of app.detail?.beats ?? []) {
        if (ms <= done) continue;
        if (ms > now + 120) break;
        click(Math.max(0, (ms - now) / 1000 / rate), n === 1);
        done = ms;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });
</script>

<footer class="deck">
  {#if track}
    <audio
      bind:this={audio}
      src={convertFileSrc(track.absPath)}
      preload="auto"
      ontimeupdate={() => !playing && (position = audio!.currentTime)}
      onplay={() => (resync(), (playing = true))}
      onseeked={resync}
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
      {#if !app.deckHere}
        <span class="from" title="Skladba zůstala na decku po přepnutí zdroje">{from}</span>
      {/if}
      {#if issue}
        <span class="pill"><span class="dot fault"></span>{app.formatOverride.get(track.id) ?? track.format}, na CDJ nepůjde</span>
      {/if}
      {#if detail?.edited}
        <span class="edited">Cue upravené, zatím jen v aplikaci <button onclick={() => app.revertCues()}>Vrátit</button></span>
      {/if}

      <div class="volume" onwheel={onVolumeWheel}>
        <button
          class="btn icon"
          onclick={() => (muted && fader === 0 ? setFader(0.7) : (muted = !muted))}
          aria-label={muted ? "Zapnout zvuk" : "Ztlumit"}
          title={muted ? "Zapnout zvuk" : "Ztlumit"}
        >
          {#if muted || fader === 0}<VolumeX size={14} />{:else if fader < 0.5}<Volume1 size={14} />{:else}<Volume2 size={14} />{/if}
        </button>
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={muted ? 0 : fader}
          oninput={(e) => setFader(Number(e.currentTarget.value))}
          style:--fill="{(muted ? 0 : fader) * 100}%"
          aria-label="Hlasitost"
          title="Hlasitost {Math.round((muted ? 0 : fader) * 100)} %"
        />
      </div>

      <div class="tools">
        <button
          class="btn small smart"
          class:on={app.smartOpen}
          onclick={() => (app.smartOpen ? (app.smartOpen = false) : app.loadSmart())}
          title="Smart Cue: najde dropy, breaky a další místa pro cue"
        >
          <Sparkles size={13} /> Smart Cue
        </button>
        <button
          class="btn small smart"
          class:on={app.gridOpen}
          onclick={() => (app.gridOpen = !app.gridOpen)}
          title="Úprava beatgridu: tempo, posun, první doba taktu, metronom"
        >
          <Grid3x3 size={13} /> Grid
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
        gridMode={app.gridOpen && app.gridEditable}
        onGridDrag={gridDrag}
      />
    </div>

    <WaveOverview wave={overview.wave} isDetail={overview.isDetail} {cues} {ghosts} {duration} {position} {span} onSeek={setTime} />

    {#if app.gridOpen}
      <GridPanel {getTime} />
    {:else if detail}
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
  .from {
    height: 22px;
    padding: 0 10px;
    border-radius: 11px;
    border: 1px solid var(--line);
    color: var(--text-2);
    font-size: 12px;
    line-height: 20px;
    white-space: nowrap;
  }
  .volume {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .volume .btn.icon {
    width: 26px;
    height: 26px;
  }
  .volume input {
    width: 92px;
    height: 4px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    appearance: none;
    background: linear-gradient(to right, var(--cue) var(--fill), var(--line) var(--fill));
    cursor: pointer;
  }
  .volume input::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.5);
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
