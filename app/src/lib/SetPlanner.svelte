<script lang="ts">
  import { Check, Headphones, LoaderCircle, Play, Route, Sparkles, Square, TrendingUp, TriangleAlert, Undo2, X } from "@lucide/svelte";
  import { convertFileSrc, invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import { cueTime, keyColor, plural, time } from "./format";
  import { PLAN_CURVES, PLAN_LEVELS, prefs } from "./prefs.svelte";
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
    technique: string;
    techniqueName: string;
    aMarks: [number, string][];
    bMarks: [number, string][];
    tips: string[];
    cost: number;
  }
  interface Plan {
    tracks: PlannedTrack[];
    leftOut: number[];
    transitions: Transition[];
    totalSeconds: number;
  }

  const LEVELS = PLAN_LEVELS;
  const CURVES = PLAN_CURVES;

  // From the settings; changed here, they hold for this plan.
  const d = prefs.value.planner;
  let minutes = $state(d.minutes);
  let level = $state(d.level);
  let curve = $state(d.curve);
  /** Keep the DJ's order (only plan transitions) or let the planner order the set. */
  let keepOrder = $state(d.keepOrder);
  /**
   * Where the transition cues go: `only` = the track keeps just A = Mix in and
   * B = Mix out (other hot cues backed up), `hot` = added on pads G/H,
   * `memory` = memory cues.
   */
  let cueMode = $state(d.cueMode);
  let backups = $state(0);
  let plan = $state<Plan | null>(null);
  let busy = $state(false);
  let applied = $state(false);
  let open = $state<number | null>(0);

  const set = $derived(app.currentPlaylist);
  const pool = $derived(new Set(set?.trackIds ?? []).size);
  const poolMinutes = $derived(Math.round((set?.trackIds ?? []).reduce((s, id) => s + (app.trackById.get(id)?.duration ?? 0), 0) / 60));
  const title = (id: number) => app.trackById.get(id)?.title ?? "?";
  const artist = (id: number) => app.trackById.get(id)?.artist ?? "";

  async function loadBackups() {
    if (set) backups = await invoke<number>("set_cue_backups", { setId: set.id }).catch(() => 0);
  }
  $effect(() => {
    void set?.id;
    loadBackups();
  });

  async function restore() {
    if (!set) return;
    await app.restoreSetCues(set.id);
    await loadBackups();
  }

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
        // Per track: mix in (from the transition into it), mix out (into the next one).
        // Per track: the cues of the transition into it and out of it.
        marks: plan.tracks.map((t, k) => [
          t.trackId,
          [
            ...(k > 0 ? plan!.transitions[k - 1].bMarks : [[t.mixInMs, "Mix in"]]),
            ...(k < plan!.transitions.length ? plan!.transitions[k].aMarks : [[t.mixOutMs, "Konec setu"]]),
          ],
        ]),
        mode: cueMode,
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
        (cueMode === "only"
          ? `${n + 1} skladeb má na padech jen cue svých přechodů (A, B, … podle času): zelená = start, růžová = předání další skladbě, červená = drop, fialová = echo out. Původní hot cue jsou zálohované (Obnovit původní cue). `
          : cueMode === "hot"
            ? `Do ${n + 1} skladeb se přidaly hot cue přechodů na nejvyšší volné pady (H, G, …). `
            : `Do ${n + 1} skladeb se zapsaly memory cue přechodů, v tabulce je poznáš podle ▾. `) +
        `Na flashku je dostaneš synchronizací.`;
    } catch (e) {
      app.error = String(e);
    } finally {
      busy = false;
    }
  }


  // ---- transition preview: both tracks over each other as planned ----
  let playingIdx = $state<number | null>(null);
  /** Playing all transitions one after another. */
  let chain = $state(false);
  let audioA: HTMLAudioElement | null = null;
  let audioB: HTMLAudioElement | null = null;
  let raf = 0;

  function stopPreview() {
    cancelAnimationFrame(raf);
    audioA?.pause();
    audioB?.pause();
    audioA = audioB = null;
    playingIdx = null;
  }
  onDestroy(stopPreview);

  function preview(i: number) {
    chain = false;
    if (playingIdx === i) return stopPreview();
    play(i);
  }

  function previewAll() {
    if (chain) {
      chain = false;
      return stopPreview();
    }
    play(0);
    chain = playingIdx != null;
  }

  function play(i: number) {
    stopPreview();
    if (!plan) return;
    const tr = plan.transitions[i];
    if (!tr) return;
    const A = app.trackById.get(tr.from);
    const B = app.trackById.get(tr.to);
    const pa = plan.tracks[i];
    const pb = plan.tracks[i + 1];
    if (!A || !B || !pa || !pb) return;
    const bar = 240 / Math.max(60, pa.bpm);
    const start = tr.startAtMs / 1000;
    // The moment the trick lands in A (a drop for swaps), else the start.
    const key = tr.aMarks.length > 1 ? tr.aMarks[1][0] / 1000 : start;
    const blend = (tr.technique === "breakdown-blend" ? 16 : tr.bars) * bar;
    const end = Math.max(key + 8 * bar, start + blend + 4 * bar);
    audioA = new Audio(convertFileSrc(A.absPath));
    audioB = new Audio(convertFileSrc(B.absPath));
    audioB.playbackRate = pa.bpm / Math.max(1, pb.bpm);
    audioB.preservesPitch = true;
    audioA.currentTime = Math.max(0, start - 8 * bar);
    audioB.currentTime = tr.cueInMs / 1000;
    audioB.volume = 0;
    audioA.play().catch((e) => (app.error = `Náhled nejde přehrát: ${e.message ?? e}`));
    playingIdx = i;
    let bStarted = false;
    const tick = () => {
      if (!audioA || !audioB) return;
      const a = audioA.currentTime;
      if (!bStarted && a >= start) {
        bStarted = true;
        audioB.play().catch(() => {});
      }
      const t = a - start;
      let va = 1;
      let vb = 0;
      if (bStarted) {
        switch (tr.technique) {
          case "drop-swap":
            vb = a < key ? 0.7 : 1;
            va = a < key ? 1 : 0;
            break;
          case "double-drop":
            vb = 1;
            va = a < key + 8 * bar ? 1 : Math.max(0, 1 - (a - key - 8 * bar) / (4 * bar));
            break;
          case "echo-out":
            vb = 1;
            va = Math.max(0, 1 - t / bar);
            break;
          default:
            vb = Math.min(1, t / (blend / 2));
            va = t < blend / 2 ? 1 : Math.max(0, 1 - (t - blend / 2) / (blend / 2));
        }
      }
      audioA.volume = Math.max(0, Math.min(1, va));
      audioB.volume = Math.max(0, Math.min(1, vb));
      if (a >= end) {
        stopPreview();
        if (chain && plan && i + 1 < plan.transitions.length) {
          open = i + 1;
          play(i + 1);
        } else chain = false;
        return;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  }

  const TECH_COLOR: Record<string, string> = {
    "long-blend": "var(--text-2)",
    "breakdown-blend": "var(--hc-f)",
    "drop-swap": "var(--hc-b)",
    "double-drop": "var(--hc-g)",
    "echo-out": "var(--hc-h)",
  };

  /** ok = harmonic and in tempo, boost = a deliberate energy key jump, warn = key or tempo off. */
  function kind(t: Transition): "ok" | "boost" | "warn" {
    if (!t.keyOk || !t.tempoOk) return "warn";
    return t.keyRelation === "+2" || t.keyRelation === "+7" ? "boost" : "ok";
  }
  const RELATION: Record<string, string> = {
    same: "stejná tónina",
    relative: "relativní",
    "+1": "+1 Camelot",
    "-1": "−1 Camelot",
    "+2": "energy boost",
    "+7": "+7 boost",
    diagonal: "diagonála",
    clash: "tónina nesedí",
    "?": "tónina neznámá",
  };
  function warnText(t: Transition) {
    if (!t.keyOk) return RELATION[t.keyRelation] ?? "tónina nesedí";
    return "tempo moc daleko";
  }
  function bpmStep(i: number) {
    if (!plan) return "";
    const d = Math.round((plan.tracks[i + 1].bpm - plan.tracks[i].bpm) * 10) / 10;
    return `${d > 0 ? "+" : d < 0 ? "−" : ""}${Math.abs(d)} BPM`;
  }

  const counts = $derived.by(() => {
    const c = { ok: 0, boost: 0, warn: 0 };
    for (const t of plan?.transitions ?? []) c[kind(t)]++;
    return c;
  });
  /** 0–100: clean transitions count fully, boosts nearly, warnings not at all. */
  const score = $derived.by(() => {
    const n = plan?.transitions.length ?? 0;
    return n ? Math.round((100 * (counts.ok + 0.85 * counts.boost)) / n) : 100;
  });
  const maxPitch = $derived(Math.max(0, ...(plan?.transitions ?? []).map((t) => Math.abs(t.pitchPercent))));
  const lenMax = $derived(Math.max(60, Math.ceil(poolMinutes / 5) * 5, minutes));
</script>

<section class="planner">
  <aside class="settings">
    <div class="shead">
      <h2><Route size={17} /> Plánovač</h2>
      <button class="btn icon ghost" onclick={() => (app.view = "tracks")} aria-label="Zavřít plánovač" title="Zavřít plánovač"><X size={15} /></button>
    </div>
    <p class="sub pool">{set?.name} · {pool} skladeb k výběru, zhruba {poolMinutes} min hudby</p>

    <div class="group">
      <span class="eyebrow">Pořadí</span>
      <div class="seg">
        <button class:on={keepOrder} onclick={() => (keepOrder = true)} title="Skladby zůstanou, jak jsi je seřadil; naplánují se jen přechody">Moje</button>
        <button class:on={!keepOrder} onclick={() => (keepOrder = false)} title="Plánovač vybere a seřadí skladby podle tóniny, tempa a energie">Navrhnout</button>
      </div>
    </div>
    <div class="group">
      <span class="eyebrow">Energie</span>
      <div class="seg">
        {#each CURVES as [v, label]}
          <button class:on={curve === v} onclick={() => (curve = v)}>{label}</button>
        {/each}
      </div>
    </div>
    <div class="group">
      <span class="eyebrow">Úroveň</span>
      <div class="seg">
        {#each LEVELS as [v, label, hint]}
          <button class:on={level === v} onclick={() => (level = v)} title={hint}>{label}</button>
        {/each}
      </div>
    </div>
    <div class="group">
      <span class="eyebrow">Cue přechodů</span>
      <div class="seg">
        <button class:on={cueMode === "only"} onclick={() => (cueMode = "only")} title="Na padech zůstanou jen cue přechodů (start, swap, drop, mix out) v pořadí A, B, …; ostatní hot cue se zazálohují a smažou, memory cue zůstanou">Jen přechody</button>
        <button class:on={cueMode === "hot"} onclick={() => (cueMode = "hot")} title="Tvoje hot cue zůstanou, cue přechodů přibudou na nejvyšší volné pady (H, G, …)">Na volné pady</button>
        <button class:on={cueMode === "memory"} onclick={() => (cueMode = "memory")} title="Jako memory cue, pady zůstanou, jak jsou">Memory</button>
      </div>
    </div>
    <label class="group len" class:off={keepOrder} title={keepOrder ? "Délka platí jen pro navržené pořadí" : "0 = všechny skladby"}>
      <span class="eyebrow">Délka</span>
      <span class="slider">
        <input type="range" min="0" max={lenMax} step="5" bind:value={minutes} disabled={keepOrder} style:--p="{(minutes / lenMax) * 100}%" />
        <span class="readout">{minutes === 0 ? "vše" : `${minutes} min`}</span>
      </span>
    </label>

    <p class="hint">
      {keepOrder ? "Tvoje pořadí zůstane, plánovač jen zkontroluje a naplánuje přechody. " : "Plánovač vybere skladby a najde nejlepší pořadí. "}{LEVELS.find((l) => l[0] === level)?.[2]}
    </p>

    <div class="sfoot">
      {#if backups > 0}
        <button class="btn ghost" onclick={restore} title="Vrátí hot cue, které skladby měly před prvním použitím plánu"><Undo2 size={14} /> Obnovit původní cue ({backups})</button>
      {/if}
      <button class="btn primary lg run" onclick={run} disabled={busy || pool < 2}>
        {#if busy && !plan}<LoaderCircle size={15} class="spin" /> Počítám…{:else}<Sparkles size={15} /> {plan ? "Naplánovat znovu" : "Naplánovat"}{/if}
      </button>
    </div>
  </aside>

  <div class="result">
    {#if !plan}
      <div class="empty">
        <Route size={28} />
        {#if pool < 2}
          <p>Do setu nejdřív vlož skladby, ze kterých má plánovač vybírat: v Celé knihovně je označ a pravým tlačítkem dej Vložit do setu.</p>
        {:else}
          <p>
            Plánovač projde {pool} skladeb (tempo, tóninu, energii a místa pro Mix in a Mix out ze Smart Cue), najde pořadí podle
            zvolené křivky energie a ke každému přechodu napíše, jak ho zahrát. Skladby s AI analýzou mají přesnější body.
          </p>
        {/if}
      </div>
    {:else}
      <div class="rhead">
        <span class="score readout" class:warn={score < 70} style:--p="{score}%">{score}</span>
        <div class="rtitle">
          <h3>
            {keepOrder ? "Tvoje pořadí" : "Návrh pořadí"}
            <span class="sub">{plan.tracks.length} skladeb · {time(plan.totalSeconds)}{#if plan.leftOut.length} · {plan.leftOut.length} se nevešlo, zůstanou na konci setu{/if}</span>
          </h3>
          <span class="sub">
            {counts.ok} z {plan.transitions.length} přechodů v harmonii · tempo ±{maxPitch.toFixed(0)} %{#if counts.boost} · {counts.boost} energy {plural(counts.boost, "boost", "boosty", "boostů")}{/if}{#if counts.warn} · {counts.warn} {plural(counts.warn, "náročný", "náročné", "náročných")}{/if}
          </span>
        </div>
        <button class="btn" onclick={previewAll} disabled={!plan.transitions.length} title="Přehraje přechody jeden po druhém: obě skladby přes sebe podle plánu">
          {#if chain}<Square size={13} fill="currentColor" /> Zastavit{:else}<Headphones size={15} /> Poslechnout přechody{/if}
        </button>
        <button class="btn primary" onclick={apply} disabled={busy || applied}>
          {#if applied}<Check size={15} /> Uloženo{:else}<Check size={15} /> {keepOrder ? "Uložit cue přechodů" : "Použít pořadí a cue"}{/if}
        </button>
      </div>

      <div class="bars" role="img" aria-label="Energie v průběhu setu, barva podle tóniny">
        {#each plan.tracks as t, i (t.trackId)}
          {@const end = i + 1 < plan.tracks.length ? plan.tracks[i + 1].start : plan.totalSeconds}
          <button
            class="bar"
            class:sel={app.selectedTrack === t.trackId}
            style:flex-grow={Math.max(1, end - t.start)}
            style:--k={keyColor(t.key)}
            style:--h="{(t.energy / 10) * 100}%"
            style:--t="{(t.target / 10) * 100}%"
            title="{title(t.trackId)} · {t.key || '?'} · energie {t.energy.toFixed(1)} (cíl {t.target.toFixed(1)})"
            onclick={() => app.selectTrack(t.trackId)}
          ><span class="fill"></span><i></i></button>
        {/each}
      </div>

      <div class="list">
        {#each plan.tracks as t, i (t.trackId)}
          <div class="track" class:sel={app.selectedTrack === t.trackId}>
            <span class="readout pos">{i + 1}</span>
            <button class="name" onclick={() => app.selectTrack(t.trackId)}>
              <span class="tt">{title(t.trackId)}</span><span class="ar">{artist(t.trackId)}</span>
            </button>
            <span class="readout at" title="Začátek v setu">{time(t.start)}</span>
            <span class="readout en" title="Energie">E {t.energy.toFixed(1)}</span>
            <span class="readout bpm">{t.bpm.toFixed(2)}</span>
            <span class="keypill" style:--k={keyColor(t.key)}>{t.key || "?"}</span>
          </div>
          {#if plan.transitions[i]}
            {@const tr = plan.transitions[i]}
            {@const k = kind(tr)}
            <div class="trans {k}">
              <button class="chip" onclick={() => (open = open === i ? null : i)} aria-expanded={open === i} title="{RELATION[tr.keyRelation] ?? tr.keyRelation} · pitch {tr.pitchPercent > 0 ? '+' : ''}{tr.pitchPercent.toFixed(1)} % · energie {tr.energyStep > 0 ? '+' : ''}{tr.energyStep.toFixed(1)}">
                {#if k === "ok"}<Check size={13} />{:else if k === "boost"}<TrendingUp size={13} />{:else}<TriangleAlert size={13} />{/if}
                <span>{t.key || "?"} → {plan.tracks[i + 1].key || "?"}</span>
                <span>·</span><span>{bpmStep(i)}</span>
                <span>·</span><span>{k === "boost" ? "energy boost" : k === "warn" ? warnText(tr) : `${tr.bars} taktů`}</span>
              </button>
              <span class="tech" style:--c={TECH_COLOR[tr.technique] ?? "var(--text-2)"}>{tr.techniqueName}</span>
              <span class="from">od {cueTime(tr.startAtMs)}</span>
              <button class="prev" class:on={playingIdx === i} onclick={() => preview(i)} title="Přehrát přechod: obě skladby přes sebe podle plánu" aria-label="Přehrát přechod">
                {#if playingIdx === i}<Square size={10} fill="currentColor" />{:else}<Play size={10} fill="currentColor" />{/if}
              </button>
            </div>
            {#if open === i}
              <ul class="tips">
                {#each tr.tips as tip}<li>{tip}</li>{/each}
                <li class="marks">
                  <span>V „{title(tr.from)}“:</span>
                  {#each tr.aMarks as [ms, label]}<span class="mk">{cueTime(ms)} {label}</span>{/each}
                </li>
                <li class="marks">
                  <span>V „{title(tr.to)}“:</span>
                  {#each tr.bMarks as [ms, label]}<span class="mk">{cueTime(ms)} {label}</span>{/each}
                </li>
              </ul>
            {/if}
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .planner {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    gap: 16px;
    padding: 4px 28px 16px;
  }
  .settings {
    flex: none;
    width: 330px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 18px 18px 18px;
    background: var(--surface-2);
    border-radius: 14px;
    overflow-y: auto;
  }
  .shead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: -4px -6px -10px 0;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  h2 :global(svg) {
    color: var(--accent);
  }
  .sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .pool {
    margin: 0;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .group .seg {
    display: flex;
  }
  .group .seg > button {
    flex: 1;
    padding: 0 4px;
    height: 30px;
    font-size: 12px;
    white-space: nowrap;
  }
  .len.off {
    opacity: 0.4;
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .slider .readout {
    min-width: 52px;
    text-align: right;
    font-size: 15px;
  }
  input[type="range"] {
    flex: 1;
    height: 4px;
    margin: 0;
    appearance: none;
    -webkit-appearance: none;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) var(--p), var(--surface-3) var(--p));
  }
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.5);
  }
  .hint {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-3);
  }
  .sfoot {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .run {
    justify-content: center;
    width: 100%;
  }
  .result {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
  }
  .empty {
    margin: auto;
    max-width: 480px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    color: var(--text-2);
    line-height: 1.5;
    text-align: center;
  }
  .empty :global(svg) {
    color: var(--text-3);
  }
  .empty p {
    margin: 0;
  }
  .rhead {
    display: flex;
    align-items: center;
    gap: 14px;
    flex: none;
  }
  .score {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    font-size: 19px;
    color: var(--success);
    --ring: var(--success);
    background:
      radial-gradient(closest-side, var(--surface-1) calc(100% - 3px), transparent calc(100% - 2px)),
      conic-gradient(var(--ring) var(--p), var(--surface-3) var(--p));
  }
  .score.warn {
    color: var(--warning);
    --ring: var(--warning);
  }
  .rtitle {
    flex: 1;
    min-width: 0;
  }
  h3 {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 0 0 2px;
    font-size: 16px;
    font-weight: 600;
  }
  h3 .sub {
    font-weight: 400;
  }
  .rtitle > .sub {
    color: var(--text-2);
  }
  .bars {
    flex: none;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 60px;
  }
  .bar {
    position: relative;
    flex-basis: 0;
    min-width: 4px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    display: flex;
    align-items: flex-end;
  }
  .fill {
    width: 100%;
    height: var(--h);
    border-radius: 3px 3px 1px 1px;
    background: color-mix(in srgb, var(--k) 62%, var(--surface-1));
  }
  .bar:hover .fill,
  .bar.sel .fill {
    background: var(--k);
  }
  /* Target energy of the curve at this track. */
  .bar i {
    position: absolute;
    left: 0;
    right: 0;
    bottom: var(--t);
    border-top: 2px dotted rgb(255 255 255 / 0.3);
    pointer-events: none;
  }
  .list {
    background: var(--surface-2);
    border-radius: 14px;
    padding: 10px 14px 12px;
  }
  .track {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto 46px 58px 42px;
    gap: 12px;
    align-items: center;
    height: 44px;
    padding: 0 8px 0 0;
    border-radius: 8px;
  }
  .track.sel {
    background: var(--surface-3);
  }
  .pos {
    color: var(--text-3);
    text-align: right;
    font-size: 13px;
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
    font-size: 13.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .ar {
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .at,
  .en {
    color: var(--text-3);
    font-size: 13px;
    text-align: right;
  }
  .bpm {
    text-align: right;
    font-size: 15px;
  }
  .trans {
    --c: var(--success);
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 0 34px;
    padding: 2px 0 2px 12px;
    border-left: 2px solid var(--c);
    font-size: 12px;
  }
  .trans.boost {
    --c: var(--accent);
  }
  .trans.warn {
    --c: var(--warning);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 10px;
    border: 0;
    border-radius: 11px;
    background: color-mix(in srgb, var(--c) 12%, transparent);
    color: var(--c);
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
  }
  .chip:hover {
    background: color-mix(in srgb, var(--c) 20%, transparent);
  }
  .tech {
    height: 22px;
    display: inline-flex;
    align-items: center;
    padding: 0 9px;
    border-radius: 11px;
    background: var(--surface-3);
    color: var(--c);
    font-size: 11.5px;
    font-weight: 500;
    white-space: nowrap;
  }
  .from {
    color: var(--text-3);
    font-family: var(--font-readout);
    font-size: 13px;
  }
  .prev {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 0;
    background: var(--surface-3);
    color: var(--text-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .prev:hover,
  .prev.on {
    color: var(--on-accent);
    background: var(--accent);
  }
  .tips {
    margin: 4px 0 8px 48px;
    padding: 8px 12px;
    list-style: none;
    background: var(--surface-1);
    border-radius: 8px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-2);
  }
  .marks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .mk {
    font-family: var(--font-readout);
    font-size: 13px;
    color: var(--text);
    background: var(--surface-3);
    border-radius: 4px;
    padding: 0 6px;
  }
</style>
