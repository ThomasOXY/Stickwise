<script lang="ts">
  import {
    AudioWaveform,
    Check,
    ChevronDown,
    CircleAlert,
    CircleCheck,
    CircleX,
    Copy,
    CheckCheck,
    Eject,
    FileX,
    Folder,
    History,
    ListMusic,
    LoaderCircle,
    ShieldCheck,
    Sparkles,
    Usb,
  } from "@lucide/svelte";
  import { api, libApi, type EngineStatus, type Playlist, type SyncPlan } from "./api";
  import { fsName, gb, plural, size, time } from "./format";
  import SheetCheck from "./SheetCheck.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import SheetSwitch from "./SheetSwitch.svelte";
  import { ENGINE, openSettings, prefs } from "./prefs.svelte";
  import { app } from "./state.svelte";

  let stick = $state<string | null>(app.sticks[0]?.path ?? null);
  let picked = $state<Set<number>>(new Set());
  let plan = $state<SyncPlan | null>(null);
  let planning = $state(false);
  let jobId = $state<number | null>(null);
  let failed = $state<string | null>(null);
  // From the settings; changed here, they hold for this sync.
  let writeSettings = $state(prefs.value.stick.writeSettings);
  /** All playlists at the top of the stick, no folders to click through on the player. */
  let flat = $state(prefs.value.stick.flat);

  // Hybrid stick: a Denon (Engine DJ) library over the same audio files.
  // A stick keeps its own choice; a new one starts from the settings.
  const engineKey = (s: string) => `syncEngine:${s.split("/").pop()}`;
  let engine = $state<string | null>(null);
  let engineStatus = $state<EngineStatus | null>(null);
  $effect(() => {
    const s = stick;
    if (!s) return;
    const own = localStorage.getItem(engineKey(s));
    engine = own === null ? prefs.value.stick.engine : own || null;
    engineStatus = null;
    libApi.engineStatus(s).then((st) => (engineStatus = st), () => {});
  });
  function setEngine(v: string | null) {
    engine = v;
    if (stick) localStorage.setItem(engineKey(stick), v ?? "");
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

  /** Tracks under a folder, for the count on its row. */
  const subtreeTracks = (p: Playlist): number =>
    p.isFolder ? children(p.id).reduce((n, c) => n + subtreeTracks(c), 0) : p.trackIds.length;
  const somePickedBelow = (p: Playlist): boolean => children(p.id).some((c) => picked.has(c.id) || somePickedBelow(c));
  function rowState(p: Playlist): "on" | "off" | "some" {
    if (picked.has(p.id) || ancestorPicked(p)) return "on";
    return p.isFolder && somePickedBelow(p) ? "some" : "off";
  }
  const pickedSets = $derived(lists.filter((p) => picked.has(p.id) && p.isSet).length);
  const pickedLists = $derived(picked.size - pickedSets);
  let safetyOpen = $state(false);

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
      await app.flushGrid();
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
    ["analysis", "Cue a waveformy"],
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
  // Space on the stick after the sync: what's used now plus what gets copied.
  const space = $derived.by(() => {
    if (!plan || !stickInfo || stickInfo.totalBytes === 0) return null;
    const total = stickInfo.totalBytes;
    const free = plan.freeBytes || stickInfo.freeBytes;
    const used = Math.max(0, total - free);
    return { total, used, add: plan.copyBytes, after: free - plan.copyBytes };
  });
  const pct = (b: number, total: number) => `${Math.min(100, Math.max(0, (b / total) * 100))}%`;
</script>

<svelte:window
  onkeydown={(e) =>
    e.key === "Escape" && job?.state !== "running" && app.settingsFor === undefined && (app.syncOpen = false)}
/>

<SheetFrame
  id="sync-title"
  icon={Usb}
  title={job?.state === "running" ? `Synchronizuji ${stickInfo?.name ?? ""}` : "Synchronizovat na flashku"}
  sub={job
    ? stickInfo?.name
      ? `Flashka ${stickInfo.name}`
      : undefined
    : "Vybrané sety a playlisty budou na flashce přesně tak, jak je máš v knihovně. Hudba se kopíruje jen tam, kde chybí."}
  width={job ? 640 : 1100}
  onclose={job?.state === "running" ? undefined : () => (app.syncOpen = false)}
>
  {#if !job}
    <div class="cols">
      <div class="left">
        <section>
          <div class="step"><span class="num">1</span><h3>Flashka</h3></div>
          {#if app.sticks.length === 0}
            <p class="none"><Usb size={14} /> Připoj flashku. Objeví se tady sama.</p>
          {:else}
            <div class="sticks">
              {#each app.sticks as s (s.path)}
                <label class="stick" class:on={stick === s.path}>
                  <input type="radio" name="stick" value={s.path} bind:group={stick} />
                  <span class="top">
                    <Usb size={15} />
                    <span class="name">{s.name}</span>
                    <span class="radio" aria-hidden="true"></span>
                  </span>
                  <span class="cap">
                    <span style:width={s.totalBytes ? pct(s.totalBytes - s.freeBytes, s.totalBytes) : "0%"}></span>
                  </span>
                  <span class="meta">{fsName(s.filesystem)} · volných {gb(s.freeBytes)} z {gb(s.totalBytes)} GB</span>
                </label>
              {/each}
            </div>
          {/if}
        </section>

        <section>
          <div class="step">
            <span class="num">2</span><h3>Co na ní bude</h3>
            <span class="count">
              {pickedSets} {plural(pickedSets, "set", "sety", "setů")} · {pickedLists}
              {plural(pickedLists, "playlist", "playlisty", "playlistů")} vybrané
            </span>
          </div>
          <div class="pick">
            {#each rows as { node, depth } (node.id)}
              {@const inherited = ancestorPicked(node)}
              {@const st = rowState(node)}
              <label class="row" class:on={st === "on"} class:inherited style:padding-left="{10 + depth * 22}px">
                <input type="checkbox" checked={st === "on"} disabled={inherited} onchange={() => toggle(node)} />
                <SheetCheck state={st} />
                {#if node.isFolder}<Folder size={14} />{:else if node.isSet}<Sparkles size={14} class="set" />{:else}<ListMusic size={14} />{/if}
                <span class="pname">{node.name}</span>
                {#if node.isSet}<span class="tag">set</span>{/if}
                {#if st === "some"}<span class="tag part">částečně</span>{/if}
                <span class="readout n">{subtreeTracks(node)}</span>
              </label>
            {/each}
          </div>
        </section>

        <section>
          <div class="step"><span class="num">3</span><h3>Možnosti</h3></div>
          <div class="opts">
            <SheetSwitch
              bind:checked={flat}
              label="Všechny playlisty rovnou nahoru, bez složek"
              desc="Na přehrávači se nemusíš proklikávat složkami. Stejné názvy dostanou do závorky jméno složky."
            />
            <SheetSwitch
              bind:checked={writeSettings}
              label="Zapsat i moje nastavení přehrávače (My Settings)"
              desc="Quantize, auto cue, jas, waveform a další, jak je máš uložené."
            >
              {#snippet extra()}
                <button class="link" onclick={(e) => (e.preventDefault(), openSettings("player"))}>upravit</button>
              {/snippet}
            </SheetSwitch>
            <SheetSwitch
              checked={!!engine}
              onchange={(on) => setEngine(on ? "3.0.1" : null)}
              label="Hybridní flashka i pro Denon"
              desc="Engine DJ knihovna nad stejnými soubory, hudba na flashce jen jednou."
            />
            {#if engine}
              <div class="engine">
                <div class="seg eng" role="radiogroup" aria-label="Nejstarší Denon">
                  {#each ENGINE as o (o.schema)}
                    <button role="radio" aria-checked={engine === o.schema} class:on={engine === o.schema} onclick={() => setEngine(o.schema)}>
                      {o.label}<span class="n">{o.note}</span>
                    </button>
                  {/each}
                </div>
                <p class="note">
                  Vyber nejstarší firmware Denonu, na kterém budeš hrát; novější ho přečtou taky. Denon si knihovnu při
                  prvním použití převede na svou verzi a starší přehrávač ji pak nepřečte, další synchronizace ji ale vrátí
                  zpátky.
                  {#if engineStatus?.version}
                    Na flashce už je Denon knihovna {engineStatus.version} ({engineStatus.tracks} skladeb): nahradí se, stará
                    půjde do zálohy.
                    {#if newer(engineStatus.version, engine)}Převedl ji novější Engine OS, zapíšu zpátky {engine}.{/if}
                  {/if}
                </p>
              </div>
            {/if}
          </div>
        </section>
      </div>

      <aside class="right">
        <div class="eyebrow">Co se stane</div>
        {#if plan}
          <div class="hero">
            <span class="readout big">{plan.tracks}</span>
            <span class="unit">{plural(plan.tracks, "skladba", "skladby", "skladeb")}</span>
          </div>
          <p class="hero-sub">v {plan.playlists} {plural(plan.playlists, "playlistu", "playlistech", "playlistech")}</p>

          <div class="facts">
            <div class="fact">
              <span class="ic"><Copy size={15} /></span>
              <span class="ft"><b>Zkopírovat</b><small>nové nebo změněné soubory</small></span>
              <span class="fv"><span class="readout">{plan.copyFiles}</span><small>{size(plan.copyBytes)}</small></span>
            </div>
            <div class="fact">
              <span class="ic ok"><CheckCheck size={15} /></span>
              <span class="ft"><b>Už na flashce</b><small>stejný soubor, nic se nekopíruje</small></span>
              <span class="fv"><span class="readout">{plan.reuseFiles}</span></span>
            </div>
            {#if plan.noAnalysis.length}
              <div class="fact">
                <span class="ic violet"><AudioWaveform size={15} /></span>
                <span class="ft"><b>Analyzovat</b><small>před zápisem: waveform, beatgrid, tónina</small></span>
                <span class="fv"><span class="readout">{plan.noAnalysis.length}</span></span>
              </div>
            {/if}
            {#if plan.missingAudio.length}
              <div class="fact">
                <span class="ic warn"><FileX size={15} /></span>
                <span class="ft"><b>Chybí na Macu</b><small>nemají soubor, vynechají se</small></span>
                <span class="fv"><span class="readout">{plan.missingAudio.length}</span></span>
              </div>
            {/if}
          </div>

          {#if space}
            <div class="space">
              <div class="space-head">
                <span>Místo na flashce po synchronizaci</span>
                <span class="readout" class:bad={!fits}>{fits ? `${gb(Math.max(0, space.after))} GB volných` : "nevejde se"}</span>
              </div>
              <div class="bar">
                <span class="used" style:width={pct(space.used, space.total)}></span>
                <span class="add" class:bad={!fits} style:width={pct(space.add, space.total)}></span>
              </div>
              <div class="legend">
                <span><i class="used"></i>obsazeno {gb(space.used)} GB</span>
                <span><i class="add" class:bad={!fits}></i>+{size(space.add)}</span>
              </div>
            </div>
          {/if}

          {#if !fits}
            <div class="callout danger"><CircleX size={15} /><span>Na flashce je málo místa ({size(plan.freeBytes)} volných).</span></div>
          {/if}
          {#if plan.missingAudio.length}
            <div class="callout">
              <CircleAlert size={15} />
              <span>{plan.missingAudio.length} skladeb nemá soubor na Macu, vynechají se.</span>
            </div>
          {/if}
          {#if plan.noAnalysis.length}
            <div class="callout">
              <CircleAlert size={15} />
              <span>{plan.noAnalysis.length} skladeb nemá analýzu: před zápisem je zanalyzuju (waveform, beatgrid, tónina).</span>
            </div>
          {/if}
        {:else if planning}
          <p class="wait"><LoaderCircle size={14} class="spin" /> Počítám, co je potřeba zkopírovat…</p>
        {:else}
          <p class="wait">Vyber flashku a aspoň jeden set nebo playlist.</p>
        {/if}
        {#if failed}<div class="callout danger"><CircleX size={15} /><span>{failed}</span></div>{/if}

        <div class="safety" class:open={safetyOpen}>
          <button class="safety-head" onclick={() => (safetyOpen = !safetyOpen)} aria-expanded={safetyOpen}>
            <ShieldCheck size={16} />
            <span>Bezpečné: nejdřív záloha, databáze se přepíše až nakonec. Vytažení flashky nic nerozbije.</span>
            <ChevronDown size={15} class="chev" />
          </button>
          {#if safetyOpen}
            <p>
              Knihovna na flashce se vymění celá: playlisty, které nevybereš, z přehrávače zmizí. Hudba na flashce zůstane,
              nic se nemaže. Stará databáze se nejdřív zazálohuje do ~/Stickwise/backups a nová se zapíše až nakonec, takže
              vytažení flashky uprostřed nic nerozbije. Historie hraní z přehrávače zůstane jen v záloze.
              {#if stickInfo?.filesystem === "exfat"}Flashka je exFAT: CDJ-2000NXS2 a starší ji nepřečtou.{/if}
              OneLibrary (CDJ-3000X, XDJ-AZ, OPUS-QUAD) zatím nezapisujeme.
            </p>
          {/if}
        </div>
      </aside>
    </div>
  {:else if job.state === "running"}
    <div class="body run">
      <div class="run-head">
        <span class="readout pct">{Math.floor(fraction * 100)} %</span>
        <span class="time">
          {#if remaining != null}zbývá <span class="readout">~{time(remaining)}</span>{:else}<span class="readout">{time(elapsed)}</span>{/if}
        </span>
      </div>
      <div class="bigbar"><div style:width="{fraction * 100}%"></div></div>
      <ol class="steps">
        {#each steps as [key, label], i (key)}
          <li class:past={i < stepIndex} class:now={i === stepIndex}>
            {#if i < stepIndex}<Check size={12} />{:else if i === stepIndex}<LoaderCircle size={12} class="spin" />{:else}<span class="pip"></span>{/if}
            {label}
          </li>
        {/each}
      </ol>
      <p class="cur">{job.current}</p>
      <div class="stats">
        {#if job.phase === "copy"}
          <span>Skladba <span class="readout">{Math.min(job.filesDone + 1, job.filesTotal)} / {job.filesTotal}</span></span>
          <span><span class="readout">{size(job.bytesDone)} / {size(job.bytesTotal)}</span></span>
          {#if speed > 0}<span><span class="readout">{(speed / 1e6).toFixed(1)}</span> MB/s</span>{/if}
        {:else if job.phase === "analysis" || job.phase === "analyze"}
          <span>Skladba <span class="readout">{job.filesDone} / {job.filesTotal}</span></span>
        {/if}
      </div>
      <p class="hint"><ShieldCheck size={14} /> Flashku nevytahuj. Kdyby se to stalo, zůstane na ní předchozí knihovna.</p>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <span class="done-ic"><CircleCheck size={30} /></span>
        <h3>Hotovo</h3>
      {:else if job.state === "cancelled"}
        <h3>Synchronizace zastavena</h3>
      {:else}
        <span class="done-ic bad"><CircleX size={30} /></span>
        <h3>Synchronizace skončila s problémem</h3>
      {/if}
      <p>{job.message}</p>
    </div>
  {/if}

  {#snippet foot()}
    {#if !job}
      {#if stickInfo}
        <span class="remember"><History size={14} /> Výběr si pamatuju pro {stickInfo.name}</span>
      {/if}
      <button class="btn ghost lg" onclick={() => (app.syncOpen = false)}>Zrušit</button>
      <button class="btn primary lg" onclick={start} disabled={!stick || !plan || picked.size === 0 || !fits}>
        <Usb size={16} />
        Synchronizovat{plan ? ` ${plan.tracks} ${plural(plan.tracks, "skladbu", "skladby", "skladeb")}` : ""}
      </button>
    {:else if job.state === "running"}
      <button class="btn lg" onclick={() => api.cancelJob(job.id)}>Zastavit</button>
    {:else}
      <button class="btn ghost lg" onclick={() => (app.syncOpen = false)}>Zavřít</button>
      {#if job.state === "done"}
        <button class="btn primary lg" onclick={eject}><Eject size={16} /> Vysunout flashku</button>
      {/if}
    {/if}
  {/snippet}
</SheetFrame>

<style>
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 376px;
  }
  .left {
    overflow-y: auto;
    padding: 22px 26px 26px;
    display: flex;
    flex-direction: column;
    gap: 26px;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }
  .num {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 600;
  }
  h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .count {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-3);
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0;
    color: var(--text-2);
  }
  .sticks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px;
  }
  .stick {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--hairline);
    cursor: pointer;
  }
  .stick:hover {
    background: var(--surface-3);
  }
  .stick.on {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .stick input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .stick:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-3);
  }
  .stick.on .top {
    color: var(--accent);
  }
  .name {
    flex: 1;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .radio {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
  }
  .stick.on .radio {
    box-shadow: inset 0 0 0 1.5px var(--accent);
    background: radial-gradient(circle, var(--accent) 0 4.5px, transparent 5px);
  }
  .cap {
    height: 4px;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .stick.on .cap {
    background: rgb(255 255 255 / 0.08);
  }
  .cap span {
    display: block;
    height: 100%;
    background: var(--text-3);
    border-radius: 2px;
  }
  .stick.on .cap span {
    background: var(--accent);
  }
  .meta {
    font-size: 12px;
    color: var(--text-3);
  }
  .pick {
    max-height: 320px;
    overflow-y: auto;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    height: 36px;
    padding-right: 12px;
    border-radius: var(--r-control);
    color: var(--text-2);
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-3);
  }
  .row.on {
    color: var(--text);
    background: rgb(255 255 255 / 0.025);
  }
  .row.inherited {
    cursor: default;
    color: var(--text-2);
  }
  .row input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .row:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .row :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  .row :global(svg.set) {
    color: var(--accent);
  }
  .pname {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    background: var(--accent-soft);
    border-radius: 6px;
    padding: 1px 7px;
  }
  .tag.part {
    color: var(--text-2);
    background: var(--surface-3);
  }
  .n {
    min-width: 26px;
    text-align: right;
    font-size: 13px;
    color: var(--text-3);
  }
  .opts {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .engine {
    margin: -4px 0 0 46px;
  }
  .seg.eng > button {
    height: auto;
    padding: 6px 12px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
  }
  .seg.eng .n {
    min-width: 0;
    text-align: left;
    font-family: var(--font-ui);
    font-size: 11px;
    font-weight: 400;
    color: var(--text-3);
  }
  .note {
    margin: 10px 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-3);
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    padding: 0 0 0 8px;
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }

  .right {
    background: var(--surface-2);
    padding: 22px 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .hero {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-top: 4px;
  }
  .big {
    font-size: 46px;
    line-height: 1;
  }
  .unit {
    font-size: 15px;
    color: var(--text-2);
  }
  .hero-sub {
    margin: -10px 0 0;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .facts {
    border-radius: var(--r-panel);
    background: var(--surface-1);
    padding: 6px 0;
  }
  .fact {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
  }
  .ic {
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: var(--r-control);
    display: grid;
    place-items: center;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .ic.ok {
    background: var(--success-soft);
    color: var(--success);
  }
  .ic.violet {
    background: rgb(155 92 240 / 0.14);
    color: var(--hc-h);
  }
  .ic.warn {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .ft {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ft b {
    font-size: 13.5px;
    font-weight: 500;
  }
  small {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .fv {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .fv .readout {
    font-size: 21px;
    line-height: 1.05;
  }
  .space {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 4px;
  }
  .space-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .space-head .readout {
    font-size: 14px;
    color: var(--text);
  }
  .bad {
    color: var(--danger) !important;
  }
  .bar {
    display: flex;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar .used {
    background: var(--text-3);
  }
  .bar .add {
    background: var(--accent);
  }
  .legend {
    display: flex;
    gap: 16px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .legend span {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .legend i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  i.used {
    background: var(--text-3);
  }
  i.add {
    background: var(--accent);
  }
  .bar .add.bad,
  i.add.bad {
    background: var(--danger);
  }
  .callout {
    display: flex;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--r-panel);
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .callout :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .callout.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .wait {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 0;
    color: var(--text-3);
  }
  .safety {
    margin-top: auto;
    padding-top: 8px;
  }
  .safety-head {
    width: 100%;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 0;
    border: 0;
    background: none;
    text-align: left;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-2);
    cursor: pointer;
  }
  .safety-head > :global(svg:first-child) {
    flex: none;
    color: var(--success);
  }
  .safety-head span {
    flex: 1;
  }
  .safety-head :global(.chev) {
    flex: none;
    color: var(--text-3);
    transition: transform 0.15s;
  }
  .safety.open .safety-head :global(.chev) {
    transform: rotate(180deg);
  }
  .safety p {
    margin: 0 0 0 26px;
    font-size: 12px;
    line-height: 1.55;
    color: var(--text-3);
  }

  .remember {
    margin-right: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-3);
  }

  .body {
    padding: 24px 26px;
    overflow-y: auto;
  }
  .run-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .pct {
    font-size: 46px;
    line-height: 1;
    color: var(--accent);
  }
  .time {
    color: var(--text-2);
  }
  .time .readout {
    font-size: 18px;
    color: var(--text);
  }
  .bigbar {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-3);
    overflow: hidden;
    margin: 14px 0 16px;
  }
  .bigbar div {
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    background: var(--surface-2);
  }
  .steps li.past {
    color: var(--text-2);
  }
  .steps li.past :global(svg) {
    color: var(--success);
  }
  .steps li.now {
    color: var(--text);
    font-weight: 600;
    background: var(--accent-soft);
  }
  .steps li.now :global(svg) {
    color: var(--accent);
  }
  .pip {
    width: 6px;
    height: 6px;
    margin: 0 3px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px var(--text-3);
  }
  .cur {
    margin: 16px 0 6px;
    font-size: 13px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .stats {
    display: flex;
    gap: 20px;
    color: var(--text-3);
  }
  .stats .readout {
    font-size: 16px;
    color: var(--text);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 18px 0 0;
    font-size: 12.5px;
    color: var(--text-3);
  }
  .hint :global(svg) {
    color: var(--success);
  }
  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    padding: 34px 26px;
  }
  .done-ic {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--success-soft);
    color: var(--success);
    margin-bottom: 4px;
  }
  .done-ic.bad {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .done h3 {
    font-size: 18px;
  }
  .done p {
    margin: 0;
    max-width: 480px;
    color: var(--text-2);
    line-height: 1.5;
  }
</style>
