<script lang="ts">
  import { ArrowDown, ArrowRight, ArrowUp, Check, LoaderCircle, Sparkles, TriangleAlert, X } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { cueTime, keyColor, time } from "./format";
  import { app } from "./state.svelte";

  interface PlannedTrack {
    trackId: number;
    start: number;
    energy: number;
    target: number;
    bpm: number;
    key: string;
    mixInMs: number;
    mixOutMs: number;
  }
  interface Transition {
    from: number;
    to: number;
    keyRelation: string;
    keyOk: boolean;
    pitchPercent: number;
    tempoOk: boolean;
    bars: number;
    startAtMs: number;
    cueInMs: number;
    energyStep: number;
    tips: string[];
    cost: number;
  }
  interface Plan {
    tracks: PlannedTrack[];
    leftOut: number[];
    transitions: Transition[];
    totalSeconds: number;
  }

  const LEVELS = [
    ["beginner", "Začátečník", "Jen harmonicky čisté páry, tempo ±3 %, dlouhé 32taktové přechody."],
    ["advanced", "Pokročilý", "Energy boosty (+2), tempo ±6 %, 16taktové přechody."],
    ["pro", "Profík", "Odvážnější tóniny a tempo, krátké 8taktové přechody, střihy."],
  ] as const;
  const CURVES = [
    ["warmup", "Warm-up"],
    ["peak", "Peak time"],
    ["journey", "Cesta"],
    ["closing", "Closing"],
  ] as const;

  let minutes = $state(Number(localStorage.getItem("plan-minutes")) || 60);
  let level = $state(localStorage.getItem("plan-level") || "advanced");
  let curve = $state(localStorage.getItem("plan-curve") || "peak");
  /** Keep the DJ's order (only plan transitions) or let the planner order the set. */
  let keepOrder = $state(localStorage.getItem("plan-order") !== "suggest");
  /** Write transition cues on pads G/H (visible on the pads and the CDJ) or as memory cues. */
  let asHot = $state(localStorage.getItem("plan-cues") !== "memory");
  let plan = $state<Plan | null>(null);
  let busy = $state(false);
  let applied = $state(false);
  let open = $state<number | null>(0);

  const set = $derived(app.currentPlaylist);
  const pool = $derived(new Set(set?.trackIds ?? []).size);
  const poolMinutes = $derived(Math.round((set?.trackIds ?? []).reduce((s, id) => s + (app.trackById.get(id)?.duration ?? 0), 0) / 60));
  const title = (id: number) => app.trackById.get(id)?.title ?? "?";
  const artist = (id: number) => app.trackById.get(id)?.artist ?? "";

  $effect(() => {
    localStorage.setItem("plan-minutes", String(minutes));
    localStorage.setItem("plan-level", level);
    localStorage.setItem("plan-curve", curve);
    localStorage.setItem("plan-order", keepOrder ? "mine" : "suggest");
    localStorage.setItem("plan-cues", asHot ? "hot" : "memory");
  });

  async function run() {
    if (!set) return;
    busy = true;
    applied = false;
    try {
      plan = await invoke<Plan>("plan_set", { setId: set.id, minutes, level, curve, keepOrder });
      open = 0;
    } catch (e) {
      app.error = String(e);
    } finally {
      busy = false;
    }
  }

  async function apply() {
    if (!set || !plan) return;
    busy = true;
    try {
      await invoke("apply_set_plan", {
        setId: set.id,
        order: plan.tracks.map((t) => t.trackId),
        leftOut: plan.leftOut,
        transitions: plan.transitions.map((t) => [t.from, t.to, t.startAtMs, t.cueInMs]),
        asHot,
      });
      await app.reloadLibrary();
      app.cues = {};
      app.memCount = {};
      applied = true;
      const n = plan.transitions.length;
      app.view = "tracks";
      // Show the new cues right away on the deck.
      const first = plan.tracks[0]?.trackId;
      if (first != null) await app.selectTrack(first, true);
      else if (app.selectedTrack != null) await app.selectTrack(app.selectedTrack, true);
      app.notice =
        (keepOrder ? `Set „${set.name}“ zůstal ve tvém pořadí. ` : `Set „${set.name}“ je přeřazený podle plánu. `) +
        (asHot
          ? `Do ${n + 1} skladeb se zapsaly hot cue přechodů: pad G = Mix in (zelený), pad H = Mix out → další skladba (růžový); kde byly obsazené, použil se jiný volný pad. `
          : `Do ${n + 1} skladeb se zapsalo ${n * 2} memory cue (Mix in a Mix out → další skladba), v tabulce je poznáš podle ▾. `) +
        `Na flashku je dostaneš synchronizací.`;
    } catch (e) {
      app.error = String(e);
    } finally {
      busy = false;
    }
  }

  // Chart geometry.
  const W = 1000;
  const H = 120;
  const total = $derived(Math.max(1, plan?.totalSeconds ?? 1));
  const x = (s: number) => (s / total) * W;
  const y = (e: number) => H - ((e - 1) / 9) * (H - 8) - 4;
  function energyColor(e: number) {
    const t = (e - 1) / 9;
    return `hsl(${220 - t * 200} 70% ${45 + t * 10}%)`;
  }
  function relationClass(t: Transition) {
    if (!t.keyOk || !t.tempoOk) return "bad";
    return ["same", "relative", "+1", "-1"].includes(t.keyRelation) ? "good" : "ok";
  }
  const RELATION: Record<string, string> = {
    same: "stejná tónina",
    relative: "relativní",
    "+1": "+1 Camelot",
    "-1": "−1 Camelot",
    "+2": "+2 energy boost",
    "+7": "+7 boost",
    diagonal: "diagonála",
    clash: "tónina se tluče",
    "?": "tónina neznámá",
  };
</script>

<section class="planner">
  <div class="bar">
    <div class="titles">
      <h2><Sparkles size={15} /> Plán setu: {set?.name}</h2>
      <span class="sub">{pool} skladeb k výběru, zhruba {poolMinutes} min hudby</span>
    </div>
    <button class="x" onclick={() => (app.view = "tracks")} aria-label="Zavřít plánovač"><X size={15} /></button>
  </div>

  <div class="params">
    <div class="group">
      <span class="lbl">Pořadí</span>
      <div class="seg">
        <button class:on={keepOrder} onclick={() => (keepOrder = true)} title="Skladby zůstanou, jak jsi je seřadil; naplánují se jen přechody">Moje pořadí</button>
        <button class:on={!keepOrder} onclick={() => (keepOrder = false)} title="Plánovač vybere a seřadí skladby podle tóniny, tempa a energie">Navrhnout pořadí</button>
      </div>
    </div>
    <div class="group">
      <span class="lbl">Cue přechodů</span>
      <div class="seg">
        <button class:on={asHot} onclick={() => (asHot = true)} title="Mix in na pad G, Mix out na pad H; obsazené pady se nepřepisují">Hot cue G/H</button>
        <button class:on={!asHot} onclick={() => (asHot = false)} title="Jako memory cue: na padech nic nezabírají">Memory cue</button>
      </div>
    </div>
    <label class="len" class:off={keepOrder}>
      <span class="lbl">Délka</span>
      <input type="number" min="0" max="600" step="5" bind:value={minutes} disabled={keepOrder} />
      <span class="sub">min {minutes === 0 ? "(vše)" : ""}</span>
    </label>
    <div class="group">
      <span class="lbl">Úroveň</span>
      <div class="seg">
        {#each LEVELS as [v, label, hint]}
          <button class:on={level === v} onclick={() => (level = v)} title={hint}>{label}</button>
        {/each}
      </div>
    </div>
    <div class="group">
      <span class="lbl">Energie</span>
      <div class="seg">
        {#each CURVES as [v, label]}
          <button class:on={curve === v} onclick={() => (curve = v)}>{label}</button>
        {/each}
      </div>
    </div>
    <button class="btn primary" onclick={run} disabled={busy || pool < 2}>
      {#if busy && !plan}<LoaderCircle size={14} class="spin" /> Počítám…{:else}<Sparkles size={14} /> {plan ? "Naplánovat znovu" : "Naplánovat"}{/if}
    </button>
  </div>
  <p class="hint">
    {keepOrder ? "Tvoje pořadí zůstane, plánovač jen zkontroluje a naplánuje přechody. " : "Plánovač vybere skladby a najde nejlepší pořadí. "}{LEVELS.find((l) => l[0] === level)?.[2]}
  </p>

  {#if !plan}
    <div class="empty">
      {#if pool < 2}
        Do setu nejdřív vlož skladby, ze kterých má plánovač vybírat: v Celé knihovně je označ a pravým tlačítkem dej Vložit do setu.
      {:else}
        Plánovač projde {pool} skladeb (tempo, tóninu, energii a místa pro Mix in a Mix out ze Smart Cue), najde pořadí podle
        zvolené křivky energie a ke každému přechodu napíše, jak ho zahrát. Skladby s AI analýzou mají přesnější body.
      {/if}
    </div>
  {:else}
    <div class="summary">
      <span><span class="readout big">{plan.tracks.length}</span> skladeb</span>
      <span><span class="readout big">{time(plan.totalSeconds)}</span> délka</span>
      <span><span class="readout big good">{plan.transitions.filter((t) => relationClass(t) === "good").length}</span> čistých přechodů</span>
      <span><span class="readout big bad">{plan.transitions.filter((t) => relationClass(t) === "bad").length}</span> náročných</span>
      {#if plan.leftOut.length}<span class="sub">{plan.leftOut.length} se nevešlo, zůstanou na konci setu</span>{/if}
      {#if keepOrder}<span class="sub">tvoje pořadí</span>{/if}
      <button class="btn primary apply" onclick={apply} disabled={busy || applied}>
        {#if applied}<Check size={14} /> Uloženo{:else}{keepOrder ? "Uložit cue přechodů" : "Použít pořadí a cue"}{/if}
      </button>
    </div>

    <svg class="chart" viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="Energie v průběhu setu">
      {#each [3, 5, 7, 9] as e}<line x1="0" x2={W} y1={y(e)} y2={y(e)} class="grid" />{/each}
      {#each plan.tracks as t, i}
        {@const end = i + 1 < plan.tracks.length ? plan.tracks[i + 1].start : plan.totalSeconds}
        <rect
          x={x(t.start) + 0.5}
          y={y(t.energy)}
          width={Math.max(1, x(end) - x(t.start) - 1)}
          height={H - y(t.energy)}
          fill={energyColor(t.energy)}
          opacity={app.selectedTrack === t.trackId ? 1 : 0.75}
          class="block"
          role="button"
          tabindex="-1"
          onclick={() => app.selectTrack(t.trackId)}
          onkeydown={() => {}}
        ><title>{title(t.trackId)} — energie {t.energy}</title></rect>
      {/each}
      <polyline
        class="target"
        points={plan.tracks.map((t) => `${x(t.start)},${y(t.target)}`).join(" ") + ` ${W},${y(plan.tracks.at(-1)?.target ?? 5)}`}
      />
    </svg>
    <div class="legend"><span class="tl"></span> cílová křivka energie <span class="sub">· výška sloupce = energie skladby (1–10), šířka = jak dlouho hraje</span></div>

    <div class="list">
      {#each plan.tracks as t, i (t.trackId)}
        <div class="track" class:sel={app.selectedTrack === t.trackId}>
          <span class="readout pos">{i + 1}</span>
          <span class="readout at">{time(t.start)}</span>
          <button class="name" onclick={() => app.selectTrack(t.trackId)}>
            <span class="tt">{title(t.trackId)}</span><span class="ar">{artist(t.trackId)}</span>
          </button>
          <span class="readout bpm">{t.bpm.toFixed(1)}</span>
          <span class="key"><i style:background={keyColor(t.key)}></i><span class="readout">{t.key || "?"}</span></span>
          <span class="en" style:color={energyColor(t.energy)}><span class="readout">{t.energy.toFixed(1)}</span></span>
        </div>
        {#if plan.transitions[i]}
          {@const tr = plan.transitions[i]}
          <button class="trans {relationClass(tr)}" onclick={() => (open = open === i ? null : i)}>
            <span class="line"></span>
            <span class="rel">{RELATION[tr.keyRelation] ?? tr.keyRelation}</span>
            <span>{tr.pitchPercent > 0 ? "+" : ""}{tr.pitchPercent.toFixed(1)} %</span>
            <span>{tr.bars} taktů od {cueTime(tr.startAtMs)}</span>
            <span class="step">
              {#if tr.energyStep > 0.4}<ArrowUp size={12} />{:else if tr.energyStep < -0.4}<ArrowDown size={12} />{:else}<ArrowRight size={12} />{/if}
              {tr.energyStep > 0 ? "+" : ""}{tr.energyStep.toFixed(1)}
            </span>
            {#if relationClass(tr) === "bad"}<TriangleAlert size={12} />{/if}
          </button>
          {#if open === i}
            <ul class="tips">
              {#each tr.tips as tip}<li>{tip}</li>{/each}
              <li class="sub">Příchozí skladbu startuj od {cueTime(tr.cueInMs)} (Mix in).</li>
            </ul>
          {/if}
        {/if}
      {/each}
    </div>
  {/if}
</section>

<style>
  .planner {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 14px 20px 0;
    overflow-y: auto;
  }
  .bar {
    display: flex;
    align-items: flex-start;
  }
  .titles {
    flex: 1;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  h2 :global(svg) {
    color: var(--cue);
  }
  .sub {
    font-size: 12px;
    color: var(--text-2);
  }
  .x {
    border: 0;
    background: none;
    color: var(--text-2);
    padding: 4px;
  }
  .params {
    display: flex;
    align-items: flex-end;
    gap: 18px;
    margin-top: 14px;
    flex-wrap: wrap;
  }
  .lbl {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
    margin-bottom: 4px;
  }
  .len.off {
    opacity: 0.4;
  }
  .len input {
    width: 70px;
    height: 28px;
    border: 1px solid var(--line);
    background: var(--booth);
    color: var(--text);
    border-radius: 6px;
    padding: 0 8px;
    font: inherit;
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
    height: 24px;
    padding: 0 10px;
    border-radius: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .seg button.on {
    background: var(--panel-2);
    color: var(--text);
  }
  .hint {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .empty {
    margin: 40px auto;
    max-width: 560px;
    color: var(--text-2);
    line-height: 1.5;
    text-align: center;
  }
  .summary {
    display: flex;
    align-items: baseline;
    gap: 18px;
    margin: 16px 0 10px;
    font-size: 12px;
    color: var(--text-2);
  }
  .big {
    font-size: 18px;
    color: var(--text);
  }
  .big.good {
    color: var(--play);
  }
  .big.bad {
    color: var(--warn);
  }
  .apply {
    margin-left: auto;
    align-self: center;
  }
  .chart {
    width: 100%;
    height: 120px;
    background: var(--booth);
    border-radius: 6px;
    flex: none;
  }
  .grid {
    stroke: var(--line);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .block {
    cursor: pointer;
  }
  .target {
    fill: none;
    stroke: var(--cue);
    stroke-width: 2;
    stroke-dasharray: 6 4;
    vector-effect: non-scaling-stroke;
  }
  .legend {
    margin: 6px 0 12px;
    font-size: 11px;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .tl {
    width: 18px;
    border-top: 2px dashed var(--cue);
  }
  .list {
    padding-bottom: 16px;
  }
  .track {
    display: grid;
    grid-template-columns: 26px 52px 1fr 54px 60px 44px;
    gap: 10px;
    align-items: center;
    height: 34px;
    padding: 0 8px;
    border-radius: 6px;
  }
  .track.sel {
    background: var(--panel-2);
  }
  .pos {
    color: var(--text-2);
    text-align: right;
  }
  .at {
    color: var(--text-2);
    font-size: 14px;
  }
  .name {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
  }
  .tt {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .ar {
    font-size: 11px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .bpm,
  .en {
    text-align: right;
    font-size: 15px;
  }
  .key {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 15px;
  }
  .key i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .trans {
    display: flex;
    align-items: center;
    gap: 14px;
    width: 100%;
    height: 24px;
    padding: 0 8px 0 44px;
    border: 0;
    background: none;
    font-size: 11px;
    color: var(--text-2);
    text-align: left;
  }
  .trans .line {
    width: 14px;
    height: 2px;
    border-radius: 1px;
    background: currentColor;
  }
  .trans.good {
    color: var(--play);
  }
  .trans.ok {
    color: var(--warn);
  }
  .trans.bad {
    color: var(--fault);
  }
  .trans .rel {
    min-width: 110px;
  }
  .trans span:not(.rel):not(.line) {
    color: var(--text-2);
  }
  .step {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .tips {
    margin: 2px 0 8px 70px;
    padding: 8px 12px;
    list-style: none;
    border-left: 2px solid var(--line);
    font-size: 12px;
    line-height: 1.6;
  }
</style>
