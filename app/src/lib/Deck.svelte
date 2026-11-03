<script lang="ts">
  import {
    ChevronDown,
    ChevronUp,
    FileAudio,
    FolderOpen,
    Grid3x3,
    Maximize2,
    Minimize2,
    Music,
    Pause,
    Play,
    Plus,
    Volume1,
    Volume2,
    VolumeX,
    WandSparkles,
    ZoomIn,
    ZoomOut,
  } from "@lucide/svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api } from "./api";
  import { cssColor } from "./cues";
  import { cueTime, keyColor, size, time } from "./format";
  import { prefs } from "./prefs.svelte";
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
  /** Collapsed: only the top row. Big: tall waveform, memory cues and the file. */
  let collapsed = $state(localStorage.getItem("deck-collapsed") === "on");
  let big = $state(localStorage.getItem("deck-big") === "on");
  let pads = $state<ReturnType<typeof CuePads>>();

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

  const memories = $derived(cues.filter((c) => c.slot === 0).sort((a, b) => a.timeMs - b.timeMs));
  const meta = $derived(
    track
      ? [track.artist, app.formatOverride.get(track.id) ?? track.format, track.fileSize ? size(track.fileSize) : ""]
          .filter(Boolean)
          .join(" · ")
      : "",
  );

  /** The last folders of a path, enough to tell where the file is. */
  function shortPath(p: string): string {
    const parts = p.split("/");
    return parts.length > 5 ? `…/${parts.slice(-4).join("/")}` : p;
  }

  $effect(() => localStorage.setItem("deck-collapsed", collapsed ? "on" : "off"));
  $effect(() => localStorage.setItem("deck-big", big ? "on" : "off"));
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

<footer class="deck" class:collapsed class:big={big && !collapsed}>
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

    <div class="top">
      <button class="play" class:on={playing} onclick={toggle} aria-label={playing ? "Pozastavit" : "Přehrát"}>
        {#if playing}<Pause size={18} fill="currentColor" />{:else}<Play size={18} fill="currentColor" />{/if}
      </button>
      <div class="art">
        {#if track.artwork}<img src={convertFileSrc(track.artwork)} alt="" />{:else}<Music size={18} />{/if}
      </div>
      <div class="names">
        <div class="t">{track.title}</div>
        <div class="a">{meta}</div>
      </div>
      <div class="flags">
        {#if !app.deckHere}
          <span class="chip" title="Skladba zůstala na decku po přepnutí zdroje">{from}</span>
        {/if}
        {#if issue}
          <span class="chip fault"><span class="dot fault"></span>{app.formatOverride.get(track.id) ?? track.format}, na CDJ nepůjde</span>
        {/if}
        {#if detail?.edited}
          <span class="chip edited">Cue upravené, zatím jen v aplikaci <button onclick={() => app.revertCues()}>Vrátit</button></span>
        {/if}
      </div>

      <div class="tools">
        {#if prefs.value.general.helpers}
          <button
            class="tool"
            class:on={app.smartOpen}
            onclick={() => (app.smartOpen ? (app.smartOpen = false) : app.loadSmart())}
            title="Smart Cue: najde dropy, breaky a další místa pro cue"
          >
            <WandSparkles size={14} /> Smart Cue
          </button>
        {/if}
        <button
          class="tool"
          class:on={app.gridOpen}
          onclick={() => (app.gridOpen = !app.gridOpen)}
          title="Úprava beatgridu: tempo, posun, první doba taktu, metronom"
        >
          <Grid3x3 size={14} /> Grid
        </button>
        {#if styles.length > 1}
          <div class="styles" role="radiogroup" aria-label="Styl waveformu">
            {#each styles as s}
              <button role="radio" aria-checked={activeStyle === s} class:on={activeStyle === s} onclick={() => (style = s)}>
                {STYLE_LABELS[s]}
              </button>
            {/each}
          </div>
        {/if}
        <button
          class="tool icon"
          onclick={() => zoom(1.5)}
          aria-label="Oddálit"
          title="Oddálit (zobrazeno {span < 10 ? span.toFixed(1) : Math.round(span)} s)"
          disabled={span >= MAX_SPAN}><ZoomOut size={14} /></button
        >
        <button
          class="tool icon"
          onclick={() => zoom(1 / 1.5)}
          aria-label="Přiblížit"
          title="Přiblížit (zobrazeno {span < 10 ? span.toFixed(1) : Math.round(span)} s)"
          disabled={span <= MIN_SPAN}><ZoomIn size={14} /></button
        >
      </div>

      <div class="readouts">
        <div class="ro">
          <span class="readout v">{track.bpm.toFixed(2)}</span>
          <span class="eyebrow">BPM</span>
        </div>
        <div class="ro">
          <span class="readout v" style:color={keyColor(track.key)}>{track.key || "–"}</span>
          <span class="eyebrow">Tónina</span>
        </div>
        <div class="ro time">
          <span class="v"><span class="readout">{time(position)}</span> <span class="readout dim">−{time(duration - position)}</span></span>
          <span class="eyebrow">Čas</span>
        </div>
      </div>

      <div class="volume" onwheel={onVolumeWheel}>
        <button
          class="vol-btn"
          onclick={() => (muted && fader === 0 ? setFader(0.7) : (muted = !muted))}
          aria-label={muted ? "Zapnout zvuk" : "Ztlumit"}
          title={muted ? "Zapnout zvuk" : "Ztlumit"}
        >
          {#if muted || fader === 0}<VolumeX size={15} />{:else if fader < 0.5}<Volume1 size={15} />{:else}<Volume2 size={15} />{/if}
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

      {#if !collapsed}
        <button class="corner" onclick={() => (big = !big)} aria-label={big ? "Zmenšit deck" : "Zvětšit deck"} title={big ? "Zmenšit deck" : "Zvětšit deck: větší waveform, memory cue a soubor"}>
          {#if big}<Minimize2 size={14} />{:else}<Maximize2 size={14} />{/if}
        </button>
      {/if}
      <button
        class="corner"
        onclick={() => (collapsed = !collapsed)}
        aria-label={collapsed ? "Rozbalit deck" : "Sbalit deck"}
        title={collapsed ? "Rozbalit deck" : "Sbalit deck"}
      >
        {#if collapsed}<ChevronUp size={16} />{:else}<ChevronDown size={16} />{/if}
      </button>
    </div>

    <!-- Hidden, not removed, when collapsed: the pads keep their keyboard shortcuts. -->
    <div class="body" hidden={collapsed}>
      {#if !collapsed}
        <div class="waves">
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
        </div>
      {/if}

      {#if app.gridOpen}
        <GridPanel {getTime} />
      {:else if detail}
        <CuePads bind:this={pads} {getTime} {setTime} beats={detail.beats} bpm={track.bpm} />
      {/if}

      {#if big && !collapsed}
        <div class="foot">
          <span class="eyebrow">Memory</span>
          <div class="mems">
            {#each memories as c}
              <button class="mem" onclick={() => setTime(c.timeMs / 1000)} title="Skok na {cueTime(c.timeMs)}">
                <i style:border-top-color={c.color ? cssColor(c) : "var(--danger)"}></i>
                <span class="readout">{cueTime(c.timeMs)}{c.loopEndMs ? ` – ${cueTime(c.loopEndMs)}` : ""}</span>
                {#if c.comment || c.loopEndMs}
                  <span class="lbl">{c.comment || `smyčka ${c.loopBeats?.[0] ?? ""}`.trim()}</span>
                {/if}
              </button>
            {/each}
            {#if pads}
              <button class="mem add" onclick={() => pads?.addMemory()} aria-label="Přidat memory cue" title="Memory cue na aktuální pozici (M)">
                <Plus size={13} />
              </button>
            {/if}
          </div>
          <span class="path" title={track.absPath}><FileAudio size={13} /> <span>{shortPath(track.absPath)}</span></span>
          <button class="btn ghost finder" onclick={() => api.reveal(track.absPath)}><FolderOpen size={13} /> Ve Finderu</button>
        </div>
      {/if}
    </div>

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
    margin: 0 28px 14px;
    padding: 14px 16px;
    border-radius: 14px;
    background: var(--surface-2);
    box-shadow:
      0 -10px 30px rgb(0 0 0 / 0.28),
      inset 0 1px 0 var(--hairline);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .deck.collapsed {
    padding: 10px 16px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  .play {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 0;
    background: var(--accent);
    color: var(--on-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    box-shadow: 0 4px 14px rgb(242 163 58 / 0.25);
  }
  .play :global(svg) {
    margin-left: 2px;
  }
  .play.on :global(svg) {
    margin-left: 0;
  }
  .play:hover {
    filter: brightness(1.08);
  }
  .art {
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: 8px;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #4a3a22, var(--surface-3));
    color: var(--text-2);
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .names {
    min-width: 0;
    flex: 0 1 auto;
  }
  .t {
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .a {
    margin-top: 2px;
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .flags {
    display: flex;
    gap: 6px;
    min-width: 0;
    flex: 0 1 auto;
    overflow: hidden;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: 12px;
    background: var(--surface-3);
    color: var(--text-2);
    font-size: 12px;
    white-space: nowrap;
    flex: none;
  }
  .chip.fault {
    background: var(--danger-soft);
    color: #f2a3a5;
  }
  .chip.edited {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.edited button {
    border: 0;
    background: none;
    padding: 0;
    color: var(--text);
    text-decoration: underline;
    font-size: 12px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
    flex: none;
    padding: 4px;
    border-radius: 10px;
    background: var(--surface-1);
  }
  .tool {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-2);
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
  }
  .tool:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .tool.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tool.icon {
    width: 28px;
    padding: 0;
    justify-content: center;
  }
  .tool:disabled {
    opacity: 0.4;
    pointer-events: none;
  }
  .styles {
    display: flex;
    gap: 2px;
    margin: 0 4px;
    padding-left: 6px;
    border-left: 1px solid var(--hairline);
  }
  .styles button {
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text-3);
    font-size: 12px;
  }
  .styles button.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .readouts {
    display: flex;
    align-items: flex-end;
    gap: 22px;
    flex: none;
    margin-left: 6px;
  }
  .ro {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }
  .ro .eyebrow {
    font-size: 10px;
    letter-spacing: 0.1em;
  }
  .ro:not(.time) {
    align-items: center;
  }
  .v,
  .v .readout {
    font-size: 26px;
    line-height: 1;
  }
  .dim {
    color: var(--text-3);
  }
  .volume {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .vol-btn {
    border: 0;
    background: none;
    padding: 2px;
    color: var(--text-2);
    display: flex;
  }
  .vol-btn:hover {
    color: var(--text);
  }
  .volume input {
    width: 72px;
    height: 4px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    appearance: none;
    background: linear-gradient(to right, var(--text) var(--fill), var(--surface-3) var(--fill));
    cursor: pointer;
  }
  .volume input::-webkit-slider-thumb {
    appearance: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.5);
  }
  .corner {
    width: 30px;
    height: 30px;
    flex: none;
    border: 0;
    border-radius: 8px;
    background: var(--surface-1);
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .corner + .corner {
    margin-left: -8px;
  }
  .corner:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .body[hidden] {
    display: none;
  }
  .waves {
    border-radius: 10px;
    overflow: hidden;
    background: #0a0b0d;
  }
  .detail {
    height: 64px;
  }
  .big .detail {
    height: 236px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .foot .eyebrow {
    flex: none;
  }
  .mems {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    max-width: 55%;
    overflow-x: auto;
  }
  .mem {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 13px;
    background: var(--surface-1);
    white-space: nowrap;
    flex: none;
  }
  .mem:hover {
    background: var(--surface-3);
  }
  .mem i {
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-top: 6px solid;
  }
  .mem .readout {
    font-size: 14px;
  }
  .mem .lbl {
    font-size: 12px;
    color: var(--text-3);
  }
  .mem.add {
    width: 26px;
    padding: 0;
    justify-content: center;
    color: var(--text-2);
    background: none;
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .path {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    font-size: 12px;
    color: var(--text-3);
  }
  .path span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .finder {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
    flex: none;
    background: var(--surface-1);
  }
  .idle {
    margin: 10px auto;
    color: var(--text-2);
  }
</style>
