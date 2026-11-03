<script lang="ts">
  import { FolderOpen, Music, Trash, X } from "@lucide/svelte";
  import type { Cue } from "./api";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { api } from "./api";
  import { cueTime, keyColor, rgb, size, time } from "./format";
  import { t } from "./i18n.svelte";
  import { app } from "./state.svelte";
  import { DETAIL_RATE, paint } from "./waveform";

  let canvas = $state<HTMLCanvasElement>();
  const track = $derived(app.selectedTrack != null ? app.trackById.get(app.selectedTrack) : undefined);
  const d = $derived(app.detail?.trackId === track?.id ? app.detail : null);
  const hot = $derived((d?.cues ?? []).filter((c) => c.slot > 0));
  const memory = $derived((d?.cues ?? []).filter((c) => c.slot === 0));
  const missingSlots = $derived(
    [1, 2, 3, 4, 5, 6, 7, 8].filter((s) => !hot.some((c) => c.slot === s)).map((s) => String.fromCharCode(64 + s)),
  );

  /** 8-second window around the first hot cue (or the first drop-ish point). */
  const windowMs = $derived.by(() => {
    const at = hot[0]?.timeMs ?? memory[0]?.timeMs ?? 60000;
    const start = Math.max(0, at - 2000);
    return [start, start + 8000] as const;
  });

  const wave = $derived(d?.details.find((w) => w.kind === localStorage.getItem("wave-style")) ?? d?.details[0] ?? null);

  $effect(() => {
    if (!canvas || !wave) return;
    paint(canvas, wave, DETAIL_RATE, windowMs[0] / 1000, windowMs[1] / 1000);
  });

  const beatsInWindow = $derived(
    (d?.beats ?? []).filter(([t]) => t >= windowMs[0] && t <= windowMs[1]),
  );
  const pos = (ms: number) => `${((ms - windowMs[0]) / (windowMs[1] - windowMs[0])) * 100}%`;
  const letter = (n: number) => String.fromCharCode(64 + n);

  function edit(c: Cue, change: Partial<Cue>) {
    const all = d?.cues ?? [];
    app.setCues(all.map((x) => (x === c ? { ...x, ...change } : x)));
  }

  function remove(c: Cue) {
    app.setCues((d?.cues ?? []).filter((x) => x !== c));
  }

  function setActive(c: Cue) {
    const all = d?.cues ?? [];
    app.setCues(all.map((x) => (x.slot === 0 ? { ...x, activeLoop: x === c ? !x.activeLoop : false } : x)));
  }
</script>

{#if track}
  <aside class="panel">
    <div class="head">
      <span class="eyebrow">{t("library.detail.eyebrow")}</span>
      <button class="btn ghost icon x" onclick={() => (app.detailOpen = false)} aria-label={t("library.detail.close")}><X size={14} /></button>
    </div>

    <div class="who">
      {#if track.artwork}
        <img src={convertFileSrc(track.artwork)} alt="" />
      {:else}
        <div class="noart"><Music size={22} /></div>
      {/if}
      <div>
        <div class="title">{track.title}</div>
        <div class="sub">{track.artist}</div>
        {#if track.album}<div class="sub">{t("library.detail.album", { album: track.album })}</div>{/if}
      </div>
    </div>

    <div class="readouts">
      <div><span class="readout big">{track.bpm.toFixed(2)}</span><span class="sub">BPM</span></div>
      <div><span class="readout big" style:color={track.key ? keyColor(track.key) : undefined}>{track.key || "–"}</span><span class="sub">{t("library.detail.key")}</span></div>
      <div><span class="readout big">{time(track.duration)}</span><span class="sub">{t("library.detail.length")}</span></div>
    </div>

    <dl>
      <dt>{t("library.detail.format")}</dt><dd>{d?.audio?.container ?? "…"}</dd>
      <dt>{t("library.detail.codec")}</dt><dd>{d?.audio ? `${d.audio.codec}${d.audio.bits ? ` ${d.audio.bits}-bit` : ""}` : "…"}</dd>
      <dt>{t("library.detail.sampleRate")}</dt><dd>{d?.audio?.sampleRate ? `${(d.audio.sampleRate / 1000).toFixed(1)} kHz` : "—"}</dd>
      <dt>{t("library.detail.size")}</dt><dd>{d ? size(d.fileSize) : "…"}</dd>
    </dl>

    <div class="sect"><span class="eyebrow">{t("library.detail.zoom")}</span><span class="readout">{cueTime(windowMs[0])} – {cueTime(windowMs[1])}</span></div>
    <div class="detail">
      <canvas bind:this={canvas}></canvas>
      {#each beatsInWindow as [t, n]}
        <span class="beat" class:down={n === 1} style:left={pos(t)}></span>
      {/each}
      {#each hot.filter((c) => c.timeMs >= windowMs[0] && c.timeMs <= windowMs[1]) as c}
        <span class="cue" style:left={pos(c.timeMs)} style:background={rgb(c.color, c.slot)}>{letter(c.slot)}</span>
      {/each}
      {#if d && !wave}<span class="none">{t("library.detail.noWaveform")}</span>{/if}
    </div>

    <div class="sect"><span class="eyebrow">{t("library.detail.hotCues")}</span></div>
    {#each hot as c (c.slot)}
      <div class="cue-row">
        <span class="badge" style:background={rgb(c.color, c.slot)}>{letter(c.slot)}</span>
        <span class="readout t">{cueTime(c.timeMs)}{c.loopEndMs ? ` – ${cueTime(c.loopEndMs)}` : ""}</span>
        <input class="cm" type="text" value={c.comment} placeholder={t("library.detail.cueName")} onchange={(e) => edit(c, { comment: e.currentTarget.value })} />
        <button class="del" onclick={() => remove(c)} aria-label={t("library.detail.deleteHotCue", { slot: letter(c.slot) })}><Trash size={12} /></button>
      </div>
    {/each}
    {#if d && missingSlots.length > 0}
      <p class="sub small">{missingSlots.length === 8 ? t("library.detail.noHotCues") : t("library.detail.unset", { slots: missingSlots.join(", ") })}</p>
    {/if}

    <div class="sect"><span class="eyebrow">{t("library.detail.memoryCues")}</span></div>
    {#each memory as c, i (`${i}:${c.timeMs}`)}
      <div class="cue-row">
        {#if c.loopEndMs}
          <button
            class="loop"
            class:active={c.activeLoop}
            style:background={rgb(c.color)}
            onclick={() => setActive(c)}
            title={c.activeLoop ? t("library.detail.activeLoop") : t("library.detail.setActiveLoop")}
          ></button>
        {:else}<span class="tri" style:border-top-color={rgb(c.color)}></span>{/if}
        <span class="readout t">{cueTime(c.timeMs)}{c.loopEndMs ? ` – ${cueTime(c.loopEndMs)}` : ""}</span>
        <input class="cm" type="text" value={c.comment} placeholder={t("library.detail.cueName")} onchange={(e) => edit(c, { comment: e.currentTarget.value })} />
        <button class="del" onclick={() => remove(c)} aria-label={t("library.detail.deleteMemoryCue")}><Trash size={12} /></button>
      </div>
    {/each}
    {#if d && memory.length === 0}<p class="sub small">{t("library.detail.noMemoryCues")}</p>{/if}

    <div class="path sub">{track.filePath}</div>
    <button class="btn" onclick={() => api.reveal(track.absPath)}><FolderOpen size={14} /> {t("library.detail.reveal")}</button>
  </aside>
{/if}

<style>
  .panel {
    width: 360px;
    flex: none;
    margin: 0 20px 16px 0;
    padding: 16px 18px 18px;
    border-radius: 16px;
    background: var(--surface-2);
    overflow-y: auto;
    animation: slide 0.16s ease-out;
  }
  @keyframes slide {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: -4px -6px 12px 0;
  }
  .x {
    width: 28px;
    height: 28px;
  }
  .who {
    display: flex;
    gap: 14px;
    align-items: center;
  }
  img,
  .noart {
    width: 72px;
    height: 72px;
    border-radius: 10px;
    object-fit: cover;
    flex: none;
  }
  .noart {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface-3);
    color: var(--text-3);
  }
  .title {
    font-size: 16px;
    font-weight: 600;
    line-height: 1.3;
    margin-bottom: 3px;
  }
  .sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .small {
    margin: 8px 0 0;
  }
  .readouts {
    display: flex;
    gap: 8px;
    margin: 16px 0 12px;
  }
  .readouts > div {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 12px;
    border-radius: var(--r-control);
    background: var(--surface-3);
  }
  .big {
    font-size: 24px;
    line-height: 1.1;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr auto 1fr;
    gap: 8px 10px;
    margin: 0;
    padding: 4px 2px;
    font-size: 12px;
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
    text-align: right;
    color: var(--text-2);
  }
  .sect {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin: 18px 0 8px;
  }
  .sect .readout {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-3);
  }
  .detail {
    position: relative;
    height: 64px;
    background: var(--bg);
    border-radius: var(--r-control);
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .beat {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: rgb(255 255 255 / 0.12);
  }
  .beat.down {
    background: rgb(255 255 255 / 0.35);
  }
  .cue {
    position: absolute;
    top: 0;
    width: 12px;
    height: 13px;
    font-size: 9px;
    font-weight: 700;
    line-height: 13px;
    text-align: center;
    color: var(--bg);
    border-radius: 0 0 3px 3px;
  }
  .none {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
  }
  .cue-row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 6px;
    border-radius: 6px;
  }
  .cue-row:hover {
    background: var(--surface-3);
  }
  .badge {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    font-size: 10.5px;
    font-weight: 700;
    color: #fff;
    text-align: center;
    line-height: 18px;
  }
  .t {
    font-size: 15px;
    min-width: 70px;
  }
  .tri {
    width: 0;
    height: 0;
    margin: 0 4px;
    border-left: 5px solid transparent;
    border-right: 5px solid transparent;
    border-top: 8px solid var(--text-2);
  }
  .loop {
    width: 18px;
    height: 8px;
    border-radius: 2px;
    border: 0;
    padding: 0;
    opacity: 0.55;
  }
  .loop.active {
    opacity: 1;
    outline: 1px solid var(--text);
    outline-offset: 1px;
  }
  .cm {
    flex: 1;
    min-width: 0;
    height: 24px;
    border-color: transparent;
    background: transparent;
    padding: 0 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .cm:hover {
    background: var(--surface-2);
  }
  .cm:focus {
    color: var(--text);
    background: var(--surface-2);
  }
  .del {
    border: 0;
    background: none;
    color: var(--text-3);
    padding: 2px;
    opacity: 0;
  }
  .cue-row:hover .del,
  .del:focus-visible {
    opacity: 1;
  }
  .del:hover {
    color: var(--danger);
  }
  .path {
    margin: 18px 0 10px;
    word-break: break-all;
  }
</style>
