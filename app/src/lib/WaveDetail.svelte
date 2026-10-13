<script lang="ts">
  import type { Cue, Waveform } from "./api";
  import { DETAIL_RATE, fit, paint } from "./waveform";

  interface Props {
    wave: Waveform | null;
    beats: [number, number][];
    cues: Cue[];
    /** Smart Cue suggestions drawn as dashed markers. */
    ghosts: { timeMs: number; label: string; color: string }[];
    duration: number;
    /** Seconds visible across the whole width. */
    span: number;
    playing: boolean;
    getTime: () => number;
    setTime: (t: number) => void;
    onZoom: (factor: number) => void;
    onScrubStart: () => void;
    onScrubEnd: () => void;
  }

  let { wave, beats, cues, ghosts, duration, span, playing, getTime, setTime, onZoom, onScrubStart, onScrubEnd }: Props =
    $props();

  let canvas = $state<HTMLCanvasElement>();
  let dirty = true;
  let lastTime = -1;
  let drag = $state<{ x: number; t: number } | null>(null);

  const HOT = ["#ee5fb0", "#e8423c", "#f28a2e", "#e6d23f", "#4cc768", "#3fcfd4", "#3e7bf2", "#9b5cf0"];
  const color = (c: Cue) =>
    c.color && (c.color[0] || c.color[1] || c.color[2])
      ? `rgb(${c.color[0]} ${c.color[1]} ${c.color[2]})`
      : c.slot > 0
        ? HOT[c.slot - 1]
        : "#e5484d";

  $effect(() => {
    // Any input change forces a repaint on the next frame.
    void [wave, beats, cues, ghosts, span, duration];
    dirty = true;
  });

  function draw() {
    if (!canvas) return;
    const t = getTime();
    const t0 = t - span / 2;
    const t1 = t + span / 2;
    const { w: W, h: H, dpr } = fit(canvas);
    let ctx: CanvasRenderingContext2D;
    if (wave) {
      ctx = paint(canvas, wave, DETAIL_RATE, t0, t1);
    } else {
      ctx = canvas.getContext("2d")!;
      ctx.clearRect(0, 0, W, H);
    }
    const X = (sec: number) => ((sec - t0) / span) * W;

    // Before the start / after the end of the track.
    ctx.fillStyle = "rgb(0 0 0 / 0.35)";
    if (t0 < 0) ctx.fillRect(0, 0, X(0), H);
    if (t1 > duration && duration > 0) ctx.fillRect(X(duration), 0, W - X(duration), H);

    // Beat grid: every beat faint, bar starts bright with a tick on top.
    const showBeats = span <= 24;
    for (const [ms, n] of beats) {
      const s = ms / 1000;
      if (s < t0 || s > t1) continue;
      if (n !== 1 && !showBeats) continue;
      const x = Math.round(X(s)) + 0.5;
      ctx.fillStyle = n === 1 ? "rgb(255 255 255 / 0.42)" : "rgb(255 255 255 / 0.14)";
      ctx.fillRect(x - 0.5, 0, Math.max(1, dpr * (n === 1 ? 1 : 0.5)), H);
      if (n === 1) {
        ctx.fillStyle = "#e5484d";
        ctx.fillRect(x - 2 * dpr, 0, 4 * dpr, 3 * dpr);
      }
    }

    // Loops: shaded span with a bar on top.
    for (const c of cues) {
      if (c.loopEndMs == null) continue;
      const a = X(c.timeMs / 1000);
      const b = X(c.loopEndMs / 1000);
      if (b < 0 || a > W) continue;
      ctx.fillStyle = color(c);
      ctx.globalAlpha = 0.18;
      ctx.fillRect(a, 0, b - a, H);
      ctx.globalAlpha = 1;
      ctx.fillRect(a, 0, b - a, 3 * dpr);
    }

    // Cue points: hot cues with a lettered tab, memory cues as a triangle.
    ctx.font = `700 ${9 * dpr}px Inter, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    for (const c of cues) {
      const x = Math.round(X(c.timeMs / 1000));
      if (x < -20 || x > W + 20) continue;
      ctx.fillStyle = color(c);
      ctx.fillRect(x, 0, Math.max(1, dpr), H);
      if (c.slot > 0) {
        const tw = 13 * dpr;
        ctx.fillRect(x, H - 13 * dpr, tw, 13 * dpr);
        ctx.fillStyle = "#14161b";
        ctx.fillText(String.fromCharCode(64 + c.slot), x + tw / 2, H - 6.5 * dpr);
      } else {
        ctx.beginPath();
        ctx.moveTo(x - 5 * dpr, H);
        ctx.lineTo(x + 5 * dpr, H);
        ctx.lineTo(x, H - 8 * dpr);
        ctx.fill();
      }
    }

    // Smart Cue suggestions: dashed line with a label on top.
    ctx.textAlign = "left";
    ctx.font = `600 ${10 * dpr}px Inter, sans-serif`;
    for (const g of ghosts) {
      const x = Math.round(X(g.timeMs / 1000));
      if (x < -80 || x > W) continue;
      ctx.fillStyle = g.color;
      for (let y = 0; y < H; y += 6 * dpr) ctx.fillRect(x, y, Math.max(1, dpr), 3 * dpr);
      const w = ctx.measureText(g.label).width + 8 * dpr;
      ctx.fillRect(x, 0, w, 14 * dpr);
      ctx.fillStyle = "#14161b";
      ctx.fillText(g.label, x + 4 * dpr, 7 * dpr);
    }

    // Fixed playhead in the centre, like rekordbox.
    ctx.fillStyle = "#f2a33a";
    ctx.fillRect(Math.round(W / 2) - dpr, 0, 2 * dpr, H);
  }

  $effect(() => {
    let raf = 0;
    const loop = () => {
      const t = getTime();
      if (dirty || playing || t !== lastTime) {
        lastTime = t;
        dirty = false;
        draw();
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    const ro = new ResizeObserver(() => (dirty = true));
    if (canvas) ro.observe(canvas);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  });

  function clamp(t: number) {
    return Math.min(Math.max(0, t), duration || 0);
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, t: getTime() };
    onScrubStart();
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag || !canvas) return;
    const dt = ((drag.x - e.clientX) / canvas.clientWidth) * span;
    setTime(clamp(drag.t + dt));
  }

  function onPointerUp() {
    if (!drag) return;
    drag = null;
    onScrubEnd();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (!canvas) return;
    // Pinch (ctrl) or vertical wheel zooms; horizontal trackpad scroll moves in time.
    if (e.ctrlKey || Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      onZoom(Math.exp(e.deltaY * (e.ctrlKey ? 0.01 : 0.002)));
    } else {
      setTime(clamp(getTime() + (e.deltaX / canvas.clientWidth) * span));
    }
  }
</script>

<canvas
  bind:this={canvas}
  class:dragging={drag}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onwheel={onWheel}
  aria-label="Waveform skladby, tažením se posouváš, kolečkem přibližuješ"
></canvas>

<style>
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
    touch-action: none;
    background: #0f1115;
    border-radius: 4px;
  }
  canvas.dragging {
    cursor: grabbing;
  }
</style>
