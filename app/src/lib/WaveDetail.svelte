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
    /** Grid edit mode: beats drawn stronger, Alt + drag moves the grid. */
    gridMode?: boolean;
    onGridDrag?: (deltaMs: number, phase: "start" | "move" | "end") => void;
  }

  let {
    wave,
    beats,
    cues,
    ghosts,
    duration,
    span,
    playing,
    getTime,
    setTime,
    onZoom,
    onScrubStart,
    onScrubEnd,
    gridMode = false,
    onGridDrag,
  }: Props = $props();
  let gridDragX: number | null = null;

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
    void [wave, beats, cues, ghosts, span, duration, gridMode];
    dirty = true;
  });

  function draw() {
    if (!canvas) return;
    const { w: W, h: H, dpr } = fit(canvas);
    // Scroll in whole device pixels: every pixel then always covers the same
    // waveform columns, so peaks do not flicker as the window slides.
    const pxPerSec = W / span;
    const t0 = Math.round((getTime() - span / 2) * pxPerSec) / pxPerSec;
    const t1 = t0 + span;
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
    // In grid mode every beat shows, stronger, with its number in the bar.
    const showBeats = gridMode || span <= 24;
    ctx.font = `700 ${9 * dpr}px Inter, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (const [ms, n] of beats) {
      const s = ms / 1000;
      if (s < t0 || s > t1) continue;
      if (n !== 1 && !showBeats) continue;
      const x = Math.round(X(s)) + 0.5;
      ctx.fillStyle = gridMode
        ? n === 1
          ? "rgb(255 255 255 / 0.75)"
          : "rgb(255 255 255 / 0.35)"
        : n === 1
          ? "rgb(255 255 255 / 0.42)"
          : "rgb(255 255 255 / 0.14)";
      ctx.fillRect(x - 0.5, 0, Math.max(1, dpr * (n === 1 ? 1 : 0.5)), H);
      if (n === 1) {
        ctx.fillStyle = "#e5484d";
        ctx.fillRect(x - 2 * dpr, 0, 4 * dpr, 3 * dpr);
      }
      if (gridMode && span <= 32) {
        ctx.fillStyle = n === 1 ? "#e5484d" : "rgb(255 255 255 / 0.6)";
        ctx.fillText(String(n), x, 5 * dpr);
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
        // Tab with the letter and, if there is room, the cue's name.
        const th = 14 * dpr;
        const letter = String.fromCharCode(64 + c.slot);
        const name = H >= 40 * dpr ? c.comment : "";
        ctx.font = `700 ${9 * dpr}px Inter, sans-serif`;
        ctx.textAlign = "left";
        const tw = (name ? ctx.measureText(name).width + 18 * dpr : 0) + 13 * dpr;
        ctx.beginPath();
        ctx.roundRect(x, H - th - 2 * dpr, tw, th, [0, 3 * dpr, 3 * dpr, 0]);
        ctx.fill();
        ctx.fillStyle = "#fff";
        ctx.fillText(letter, x + 4 * dpr, H - th / 2 - 2 * dpr);
        if (name) {
          ctx.font = `500 ${9 * dpr}px Inter, sans-serif`;
          ctx.fillText(name, x + 14 * dpr, H - th / 2 - 2 * dpr);
        }
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
    const px = Math.round(W / 2);
    ctx.fillStyle = "#f4f4f5";
    ctx.fillRect(px - dpr, 0, 2 * dpr, H);
    ctx.beginPath();
    ctx.moveTo(px - 5 * dpr, 0);
    ctx.lineTo(px + 5 * dpr, 0);
    ctx.lineTo(px, 6 * dpr);
    ctx.fill();
    ctx.fillStyle = "#e5484d";
    ctx.beginPath();
    ctx.moveTo(px - 5 * dpr, H);
    ctx.lineTo(px + 5 * dpr, H);
    ctx.lineTo(px, H - 6 * dpr);
    ctx.fill();
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
    if (gridMode && e.altKey && onGridDrag) {
      gridDragX = e.clientX;
      onGridDrag(0, "start");
      return;
    }
    drag = { x: e.clientX, t: getTime() };
    onScrubStart();
  }

  const gridDelta = (e: PointerEvent) => ((e.clientX - gridDragX!) / canvas!.clientWidth) * span * 1000;

  function onPointerMove(e: PointerEvent) {
    if (gridDragX != null && canvas) {
      onGridDrag?.(gridDelta(e), "move");
      return;
    }
    if (!drag || !canvas) return;
    const dt = ((drag.x - e.clientX) / canvas.clientWidth) * span;
    setTime(clamp(drag.t + dt));
  }

  function onPointerUp(e: PointerEvent) {
    if (gridDragX != null && canvas) {
      onGridDrag?.(gridDelta(e), "end");
      gridDragX = null;
      return;
    }
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
    background: #0a0b0d;
  }
  canvas.dragging {
    cursor: grabbing;
  }
</style>
