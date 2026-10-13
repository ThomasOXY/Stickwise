<script lang="ts">
  import { ChevronDown, ChevronRight, LoaderCircle, Lock } from "@lucide/svelte";
  import { api } from "./api";
  import { app } from "./state.svelte";

  let showWarnings = $state(false);
  const r = $derived(app.verify);
  const errors = $derived(r?.issues.filter((i) => i.severity === "error") ?? []);
  const warnings = $derived(r?.issues.filter((i) => i.severity === "warning") ?? []);

  function result(p: NonNullable<typeof r>["players"][number]) {
    if (!p.filesystemOk) return { cls: "fault", text: "Flashku nepřečte" };
    if (p.unplayable.length > 0) return { cls: "warn", text: `Pojede, ${p.unplayable.length} skladeb ne` };
    return { cls: "ok", text: "Pojede všechno" };
  }

  function abs(devicePath: string) {
    return `${app.stickPath}${devicePath}`;
  }
</script>

<section class="health">
  {#if !r}
    <div class="loading">
      <LoaderCircle size={18} class="spin" />
      Kontroluji skladby {app.verifyProgress[0]} / {app.verifyProgress[1]}
    </div>
  {:else}
    <div class="summary">
      <div><span class="readout n">{r.tracks}</span><span>skladeb</span></div>
      <div><span class="readout n fault">{errors.length}</span><span>chyb</span></div>
      <div><span class="readout n warn">{warnings.length}</span><span>varování</span></div>
      <div><span class="readout n dim">{r.appleDouble}</span><span>souborů ._</span></div>
      <div class="when">
        <div>Kontrola flashky</div>
        <div class="sub">Trvala {r.seconds.toFixed(0)} s</div>
      </div>
    </div>

    <h3>Přehrávače</h3>
    <div class="grid players">
      <span class="th">Přehrávač</span><span class="th">Souborový systém</span><span class="th">Formáty</span><span class="th">Výsledek</span>
      {#each r.players as p}
        {@const res = result(p)}
        <span class="strong">{p.player}</span>
        <span><span class="dot {p.filesystemOk ? 'ok' : 'fault'}"></span>{p.filesystemNote}</span>
        <span><span class="dot {p.formatNote ? 'fault' : 'ok'}"></span>{p.formatNote || "Všechny formáty pojedou"}</span>
        <span class="strong"><span class="dot {res.cls}"></span>{res.text}</span>
      {/each}
    </div>

    <h3>Chyby <span class="count fault">{errors.length}</span></h3>
    {#if errors.length === 0}
      <p class="none">Žádná skladba nemá chybu.</p>
    {/if}
    <div class="issues">
      {#each errors as i (i.trackId + i.kind)}
        {@const t = app.trackById.get(i.trackId)}
        <div class="issue">
          <span class="dot fault"></span>
          <div class="who">
            <div class="strong">{t?.title ?? i.title}</div>
            <div class="sub">{t?.artist ?? ""}</div>
          </div>
          <div class="what">
            <div>{i.message}</div>
            <div class="sub">{i.playlists.length ? `V playlistech: ${i.playlists.join(", ")}` : "Není v žádném playlistu"}</div>
          </div>
          <button class="btn" onclick={() => api.reveal(abs(i.filePath))}>Ukázat ve Finderu</button>
          <button class="btn" disabled title="Brzy">Opravit…</button>
        </div>
      {/each}
    </div>

    <div class="bottom">
      <div class="box">
        <button class="toggle" onclick={() => (showWarnings = !showWarnings)}>
          {#if showWarnings}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
          <span class="strong">Varování</span><span class="count warn">{warnings.length}</span>
        </button>
        <p class="sub">
          {warnings.length} skladeb má v databázi jinou velikost souboru než ve skutečnosti. Hrát půjdou.
        </p>
        {#if showWarnings}
          <ul>
            {#each warnings as w (w.trackId)}
              <li><span>{w.title}</span><span class="sub">{w.message}</span></li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="box">
        <div><span class="strong">Úklid</span> <span class="count">{r.appleDouble}</span></div>
        <p class="sub">{r.appleDouble} souborů ._ od macOS. Zbytečně zabírají místo a na CDJ vypadají jako rozbité skladby.</p>
      </div>
    </div>
  {/if}
  <footer><Lock size={12} /> Kontrola jen četla, na flashku se nic nezapsalo.</footer>
</section>

<style>
  .health {
    flex: 1;
    overflow-y: auto;
    padding: 18px 24px 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .loading {
    margin: auto;
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-2);
  }
  .summary {
    display: flex;
    align-items: flex-end;
    gap: 36px;
    margin-bottom: 18px;
  }
  .summary > div:not(.when) {
    display: flex;
    flex-direction: column;
    font-size: 11px;
    color: var(--text-2);
  }
  .n {
    font-size: 30px;
    line-height: 1;
    color: var(--text);
  }
  .n.fault,
  .count.fault {
    color: var(--fault);
  }
  .n.warn,
  .count.warn {
    color: var(--warn);
  }
  .n.dim {
    color: var(--text-2);
  }
  .when {
    margin-left: auto;
    text-align: right;
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
    color: var(--text-2);
    font-weight: 400;
  }
  h3 {
    margin: 10px 0 8px;
    font-size: 13px;
    font-weight: 600;
  }
  .count {
    font-family: var(--font-readout);
    font-weight: 600;
    margin-left: 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: 140px 1fr 1fr 1fr;
    margin-bottom: 14px;
  }
  .grid > span {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    border-bottom: 1px solid var(--line);
    color: var(--text-2);
  }
  .grid .th {
    font-size: 11px;
    height: 24px;
  }
  .grid .strong {
    color: var(--text);
  }
  .strong {
    font-weight: 500;
    color: var(--text);
  }
  .none {
    color: var(--text-2);
  }
  .issue {
    display: grid;
    grid-template-columns: 8px minmax(200px, 1fr) minmax(260px, 1.3fr) auto auto;
    gap: 14px;
    align-items: center;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }
  .bottom {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    margin: 18px 0;
  }
  .box {
    background: var(--panel);
    border-radius: 6px;
    padding: 12px 16px;
  }
  .box p {
    margin: 6px 0 0;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    padding: 0;
  }
  ul {
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
    max-height: 220px;
    overflow-y: auto;
  }
  li {
    display: flex;
    flex-direction: column;
    padding: 5px 0;
    border-top: 1px solid var(--line);
  }
  footer {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--text-2);
  }
</style>
