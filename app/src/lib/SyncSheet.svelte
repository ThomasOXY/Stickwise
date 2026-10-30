<script lang="ts">
  import { Check, CircleCheck, Eject, Folder, ListMusic, LoaderCircle, Usb } from "@lucide/svelte";
  import { api, libApi, type EngineStatus, type Playlist, type SyncPlan } from "./api";
  import { fsName, gb, plural, size, time } from "./format";
  import { app } from "./state.svelte";

  let stick = $state<string | null>(app.sticks[0]?.path ?? null);
  let picked = $state<Set<number>>(new Set());
  let plan = $state<SyncPlan | null>(null);
  let planning = $state(false);
  let jobId = $state<number | null>(null);
  let failed = $state<string | null>(null);
  let writeSettings = $state(localStorage.getItem("syncSettings") !== "off");
  $effect(() => localStorage.setItem("syncSettings", writeSettings ? "on" : "off"));
  /** All playlists at the top of the stick, no folders to click through on the player. */
  let flat = $state(localStorage.getItem("syncFlat") !== "off");
  $effect(() => localStorage.setItem("syncFlat", flat ? "on" : "off"));

  // Hybrid stick: a Denon (Engine DJ) library over the same audio files.
  const ENGINE: { schema: string; label: string; note: string }[] = [
    { schema: "2.21.0", label: "Engine OS 3.1+", note: "i nejstarší Denony" },
    { schema: "3.0.1", label: "Engine OS 4.3+", note: "doporučeno" },
    { schema: "3.0.2", label: "Engine OS 5+", note: "jen nejnovější" },
  ];
  const engineKey = (s: string) => `syncEngine:${s.split("/").pop()}`;
  let engine = $state<string | null>(null);
  let engineStatus = $state<EngineStatus | null>(null);
  $effect(() => {
    const s = stick;
    if (!s) return;
    engine = localStorage.getItem(engineKey(s)) || null;
    engineStatus = null;
    libApi.engineStatus(s).then((st) => (engineStatus = st), () => {});
  });
  function setEngine(v: string | null) {
    engine = v;
    if (stick) v ? localStorage.setItem(engineKey(stick), v) : localStorage.removeItem(engineKey(stick));
  }
  const newer = (a: string, b: string) => {
    const [x, y] = [a, b].map((v) => v.split(".").map(Number));
    for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
    return false;
  };

  const lists = $derived(app.library?.playlists ?? []);
  const job = $derived(jobId != null ? app.jobs.find((j) => j.id === jobId) : undefined);
  const stickInfo = $derived(app.sticks.find((s) => s.path === stick));

  // Last selection for this stick, or the sets by default.
  $effect(() => {
    const s = stick;
    if (!s) return;
    libApi.syncSelection(s).then((ids) => {
      const valid = ids.filter((id) => lists.some((p) => p.id === id));
      picked = new Set(valid.length ? valid : lists.filter((p) => p.isSet).map((p) => p.id));
    });
  });

  // Recompute the plan when the selection changes.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const s = stick;
    const ids = [...picked];
    clearTimeout(timer);
    plan = null;
    if (!s || ids.length === 0) return;
    planning = true;
    timer = setTimeout(async () => {
      try {
        plan = await libApi.syncPlan(s, ids);
      } catch (e) {
        failed = String(e);
      } finally {
        planning = false;
      }
    }, 250);
  });

  function children(parent: number): Playlist[] {
    return lists.filter((p) => p.parentId === parent).sort((a, b) => a.sortOrder - b.sortOrder);
  }
  const rows = $derived.by(() => {
    const out: { node: Playlist; depth: number }[] = [];
    const walk = (parent: number, depth: number) => {
      for (const n of children(parent)) {
        out.push({ node: n, depth });
        if (n.isFolder) walk(n.id, depth + 1);
      }
    };
    walk(0, 0);
    // Sets first, the way the tree shows them.
    return [...out.filter((r) => r.node.isSet), ...out.filter((r) => !r.node.isSet)];
  });
  const ancestorPicked = (p: Playlist): boolean => {
    let parent = p.parentId;
    while (parent) {
      if (picked.has(parent)) return true;
      parent = lists.find((x) => x.id === parent)?.parentId ?? 0;
    }
    return false;
  };

  function toggle(p: Playlist) {
    const next = new Set(picked);
    next.has(p.id) ? next.delete(p.id) : next.add(p.id);
    picked = next;
  }

  async function start() {
    if (!stick) return;
    failed = null;
    try {
      started = performance.now();
      jobId = await libApi.syncStart(stick, [...picked], writeSettings, engine, flat);
    } catch (e) {
      failed = String(e);
    }
  }

  async function eject() {
    if (!stick) return;
    try {
      await api.eject(stick);
      app.syncOpen = false;
      app.notice = "Flashka je vysunutá, můžeš ji vytáhnout.";
    } catch (e) {
      failed = `Flashku nejde vysunout: ${e}`;
    }
  }

  // Progress: the steps, how far the whole sync is, copy speed and time left.
  const STEPS: [string, string][] = [
    ["analyze", "Analýza"],
    ["backup", "Záloha"],
    ["copy", "Kopírování"],
    ["analysis", "Analýzy a cue"],
    ["database", "Databáze"],
    ["settings", "Nastavení"],
    ["engine", "Denon"],
    ["verify", "Kontrola"],
  ];
  const steps = $derived(
    STEPS.filter(
      ([k]) =>
        (k !== "settings" || writeSettings) && (k !== "engine" || engine) && (k !== "analyze" || (plan?.noAnalysis.length ?? 0) > 0),
    ),
  );
  let started = 0;
  let now = $state(performance.now());
  let copyFrom: { t: number; bytes: number } | null = null;
  let speed = $state(0);
  $effect(() => {
    if (job?.state !== "running") return;
    const t = setInterval(() => (now = performance.now()), 500);
    return () => clearInterval(t);
  });
  $effect(() => {
    if (!job || job.phase !== "copy") return;
    const t = performance.now();
    if (!copyFrom) copyFrom = { t, bytes: job.bytesDone };
    else if (t - copyFrom.t > 1000) speed = ((job.bytesDone - copyFrom.bytes) / (t - copyFrom.t)) * 1000;
  });
  const stepIndex = $derived(job ? steps.findIndex(([k]) => k === job.phase) : -1);
  const fraction = $derived(job?.fraction ?? 0);
  const elapsed = $derived(Math.max(0, (now - started) / 1000));
  const remaining = $derived(fraction > 0.03 && elapsed > 3 ? (elapsed * (1 - fraction)) / fraction : null);

  const fits = $derived(!plan || plan.freeBytes === 0 || plan.copyBytes + 64e6 < plan.freeBytes);
</script>

<svelte:window
  onkeydown={(e) =>
    e.key === "Escape" && job?.state !== "running" && app.settingsFor === undefined && (app.syncOpen = false)}
/>

<div class="dim"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="sync-title">
  {#if !job}
    <div class="body">
      <h2 id="sync-title">Synchronizovat na flashku</h2>
      <p class="sub">Vybrané sety a playlisty budou na flashce přesně tak, jak je máš v knihovně.</p>

      <h3>Flashka</h3>
      {#if app.sticks.length === 0}
        <p class="none"><Usb size={14} /> Připoj flashku. Objeví se tady sama.</p>
      {:else}
        <div class="sticks">
          {#each app.sticks as s (s.path)}
            <label class="stick" class:on={stick === s.path}>
              <input type="radio" name="stick" value={s.path} bind:group={stick} />
              <span class="name">{s.name}</span>
              <span class="meta">{fsName(s.filesystem)}, volných {gb(s.freeBytes)} z {gb(s.totalBytes)} GB</span>
            </label>
          {/each}
        </div>
      {/if}

      <h3>Co na ní bude</h3>
      <div class="pick">
        {#each rows as { node, depth } (node.id)}
          {@const inherited = ancestorPicked(node)}
          <label class="row" style:padding-left="{10 + depth * 18}px" class:dim-row={inherited}>
            <input type="checkbox" checked={picked.has(node.id) || inherited} disabled={inherited} onchange={() => toggle(node)} />
            {#if node.isFolder}<Folder size={13} />{:else}<ListMusic size={13} class={node.isSet ? "set" : ""} />{/if}
            <span class="pname">{node.name}</span>
            {#if node.isSet}<span class="tag">set</span>{/if}
            {#if !node.isFolder}<span class="readout n">{node.trackIds.length}</span>{/if}
          </label>
        {/each}
      </div>

      {#if plan}
        <div class="plan">
          <div><span class="readout big">{plan.tracks}</span> {plural(plan.tracks, "skladba", "skladby", "skladeb")} v {plan.playlists} {plural(plan.playlists, "playlistu", "playlistech", "playlistech")}</div>
          <div><span class="readout big">{plan.copyFiles}</span> zkopírovat ({size(plan.copyBytes)}), <span class="readout">{plan.reuseFiles}</span> už na flashce je</div>
          {#if !fits}<div class="err">Na flashce je málo místa ({size(plan.freeBytes)} volných).</div>{/if}
          {#if plan.missingAudio.length}<div class="warn">{plan.missingAudio.length} skladeb nemá soubor na Macu, vynechají se.</div>{/if}
          {#if plan.noAnalysis.length}<div class="warn">{plan.noAnalysis.length} skladeb nemá analýzu: před zápisem je zanalyzuju (waveform, beatgrid, tónina).</div>{/if}
        </div>
      {:else if planning}
        <p class="sub">Počítám, co je potřeba zkopírovat…</p>
      {/if}

      <p class="note">
        Knihovna na flashce se vymění celá: playlisty, které nevybereš, z přehrávače zmizí. Hudba na flashce zůstane,
        nic se nemaže. Stará databáze se nejdřív zazálohuje do ~/RB-EXPORT/backups a nová se zapíše až nakonec,
        takže vytažení flashky uprostřed nic nerozbije. Historie hraní z přehrávače zůstane jen v záloze.
        {#if stickInfo?.filesystem === "exfat"}Flashka je exFAT: CDJ-2000NXS2 a starší ji nepřečtou.{/if}
        OneLibrary (CDJ-3000X, XDJ-AZ, OPUS-QUAD) zatím nezapisujeme.
      </p>
      <div class="engine">
        <label class="opt">
          <input type="checkbox" checked={!!engine} onchange={(e) => setEngine(e.currentTarget.checked ? "3.0.1" : null)} />
          <span>Hybridní flashka: i pro Denon (Engine DJ), hudba na flashce jen jednou</span>
        </label>
        {#if engine}
          <div class="eng-seg" role="radiogroup" aria-label="Nejstarší Denon">
            {#each ENGINE as o (o.schema)}
              <button role="radio" aria-checked={engine === o.schema} class:on={engine === o.schema} onclick={() => setEngine(o.schema)}>
                <span class="t">{o.label}</span><span class="n">{o.note}</span>
              </button>
            {/each}
          </div>
          <p class="note">
            Vyber nejstarší firmware Denonu, na kterém budeš hrát; novější ho přečtou taky. Denon si knihovnu při prvním
            použití převede na svou verzi a starší přehrávač ji pak nepřečte, další synchronizace ji ale vrátí zpátky.
            {#if engineStatus?.version}
              Na flashce už je Denon knihovna {engineStatus.version} ({engineStatus.tracks} skladeb): nahradí se, stará půjde do zálohy.
              {#if newer(engineStatus.version, engine)}Převedl ji novější Engine OS, zapíšu zpátky {engine}.{/if}
            {/if}
          </p>
        {/if}
      </div>
      <label class="opt">
        <input type="checkbox" bind:checked={flat} />
        <span>
          Všechny playlisty rovnou nahoru, bez složek
          <span class="sub">Na přehrávači se nemusíš proklikávat složkami. Stejné názvy dostanou do závorky jméno složky.</span>
        </span>
      </label>
      <label class="opt">
        <input type="checkbox" bind:checked={writeSettings} />
        <span>
          Zapsat i moje nastavení přehrávače (My Settings)
          <button class="link" onclick={(e) => (e.preventDefault(), (app.settingsFor = null))}>upravit</button>
        </span>
      </label>
      {#if failed}<p class="err">{failed}</p>{/if}
    </div>
    <div class="foot">
      <button class="btn" onclick={() => (app.syncOpen = false)}>Zrušit</button>
      <button class="btn primary" onclick={start} disabled={!stick || !plan || picked.size === 0 || !fits}>
        Synchronizovat{plan ? ` ${plan.tracks} ${plural(plan.tracks, "skladbu", "skladby", "skladeb")}` : ""}
      </button>
    </div>
  {:else if job.state === "running"}
    <div class="body">
      <div class="head">
        <h2>Synchronizuji {stickInfo?.name ?? ""}</h2>
        <span class="readout pct">{Math.floor(fraction * 100)} %</span>
      </div>
      <ol class="steps">
        {#each steps as [key, label], i (key)}
          <li class:past={i < stepIndex} class:now={i === stepIndex}>
            {#if i < stepIndex}<Check size={12} />{:else if i === stepIndex}<LoaderCircle size={12} class="spin" />{:else}<span class="pip"></span>{/if}
            {label}
          </li>
        {/each}
      </ol>
      <div class="bigbar"><div style:width="{fraction * 100}%"></div></div>
      <p class="sub cur">{job.current}</p>
      <div class="stats">
        {#if job.phase === "copy"}
          <span>Skladba <span class="readout">{Math.min(job.filesDone + 1, job.filesTotal)} / {job.filesTotal}</span></span>
          <span><span class="readout">{size(job.bytesDone)} / {size(job.bytesTotal)}</span></span>
          {#if speed > 0}<span><span class="readout">{(speed / 1e6).toFixed(1)}</span> MB/s</span>{/if}
        {:else if job.phase === "analysis" || job.phase === "analyze"}
          <span>Skladba <span class="readout">{job.filesDone} / {job.filesTotal}</span></span>
        {:else}
          <span></span>
        {/if}
        <span>
          {#if remaining != null}zbývá <span class="readout">~{time(remaining)}</span>{:else}<span class="readout">{time(elapsed)}</span>{/if}
        </span>
      </div>
      <p class="sub">Flashku nevytahuj. Kdyby se to stalo, zůstane na ní předchozí knihovna.</p>
    </div>
    <div class="foot">
      <button class="btn" onclick={() => api.cancelJob(job.id)}>Zastavit</button>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <CircleCheck size={36} color="var(--play)" />
        <h2>Hotovo</h2>
      {:else if job.state === "cancelled"}
        <h2>Synchronizace zastavena</h2>
      {:else}
        <h2>Synchronizace skončila s problémem</h2>
      {/if}
      <p>{job.message}</p>
    </div>
    <div class="foot">
      <button class="btn" onclick={() => (app.syncOpen = false)}>Zavřít</button>
      {#if job.state === "done"}
        <button class="btn primary" onclick={eject}><Eject size={14} /> Vysunout flashku</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .dim {
    position: fixed;
    inset: 0;
    background: rgb(10 11 14 / 0.55);
    z-index: 20;
  }
  .sheet {
    position: fixed;
    top: 36px;
    left: 50%;
    transform: translateX(-50%);
    width: 600px;
    max-height: calc(100vh - 72px);
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 24px 60px rgb(0 0 0 / 0.5);
    z-index: 21;
    animation: drop 0.18s ease-out;
  }
  @keyframes drop {
    from {
      transform: translate(-50%, -12px);
      opacity: 0;
    }
  }
  .body {
    padding: 20px;
    overflow-y: auto;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--text-2);
  }
  .sticks {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .stick {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .stick.on {
    border-color: var(--cue);
  }
  .stick input {
    accent-color: var(--cue);
    margin: 0;
  }
  .stick .name {
    font-weight: 600;
  }
  .stick .meta {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-2);
  }
  .pick {
    max-height: 220px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--booth);
    padding: 4px 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding-right: 12px;
    color: var(--text);
  }
  .row input {
    accent-color: var(--cue);
    margin: 0;
  }
  .dim-row {
    color: var(--text-2);
  }
  .row :global(.set) {
    color: var(--cue);
  }
  .pname {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    font-size: 10px;
    color: var(--cue);
    border: 1px solid rgb(242 163 58 / 0.5);
    border-radius: 3px;
    padding: 0 4px;
  }
  .n {
    font-size: 12px;
    color: var(--text-2);
  }
  .plan {
    margin-top: 14px;
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--booth);
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .big {
    font-size: 17px;
    color: var(--text);
  }
  .warn {
    color: var(--warn);
  }
  .err {
    color: var(--fault);
  }
  .note {
    margin: 14px 0 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .pct {
    font-size: 22px;
    color: var(--cue);
  }
  .steps {
    display: flex;
    gap: 14px;
    list-style: none;
    margin: 14px 0 0;
    padding: 0;
    font-size: 11px;
    color: var(--text-3);
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .steps li.past {
    color: var(--text-2);
  }
  .steps li.past :global(svg) {
    color: var(--play);
  }
  .steps li.now {
    color: var(--text);
    font-weight: 600;
  }
  .steps li.now :global(svg) {
    color: var(--cue);
  }
  .pip {
    width: 6px;
    height: 6px;
    margin: 0 3px;
    border-radius: 50%;
    border: 1px solid var(--text-3);
  }
  .cur {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-bottom: 8px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
    font-size: 12px;
  }
  .opt .sub {
    display: block;
    margin: 2px 0 0;
  }
  .engine {
    margin-top: 12px;
    padding-top: 2px;
  }
  .engine .opt {
    margin-top: 0;
  }
  .eng-seg {
    display: flex;
    margin: 8px 0 0 25px;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
    background: var(--booth);
  }
  .eng-seg button {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    padding: 5px 4px;
    border: 0;
    border-right: 1px solid var(--line);
    background: none;
    color: var(--text-2);
    font-size: 12px;
  }
  .eng-seg button:last-child {
    border-right: 0;
  }
  .eng-seg button.on {
    background: var(--panel-2);
    color: var(--cue);
    font-weight: 600;
  }
  .eng-seg .n {
    font-size: 10px;
    font-weight: 400;
    color: var(--text-3);
  }
  .engine .note {
    margin: 8px 0 0 25px;
  }
  .opt input {
    accent-color: var(--cue);
    width: 15px;
    height: 15px;
    margin: 0;
  }
  .link {
    border: 0;
    background: none;
    color: var(--cue);
    padding: 0 0 0 4px;
    font: inherit;
    text-decoration: underline;
  }
  .bigbar {
    height: 8px;
    border-radius: 4px;
    background: var(--line);
    overflow: hidden;
    margin: 14px 0 10px;
  }
  .bigbar div {
    height: 100%;
    background: var(--cue);
    transition: width 0.15s linear;
  }
  .stats {
    display: flex;
    justify-content: space-between;
    color: var(--text-2);
  }
  .stats .readout {
    font-size: 16px;
    color: var(--text);
  }
  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
    padding: 30px 20px;
  }
  .done p {
    max-width: 480px;
    color: var(--text-2);
  }
</style>
