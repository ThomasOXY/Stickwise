<script lang="ts">
  import {
    Brush,
    ChevronDown,
    CircleCheck,
    CircleX,
    Disc3,
    FileX2,
    FolderSearch,
    ListMusic,
    LoaderCircle,
    Lock,
    TriangleAlert,
    Wrench,
  } from "@lucide/svelte";
  import { api } from "./api";
  import { fsName, plural } from "./format";
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
    <div class="stats">
      <div class="stat">
        <div class="head">Skladeb <ListMusic size={15} /></div>
        <div class="val"><span class="readout n">{r.tracks}</span><span class="sub">zkontrolováno za {r.seconds.toFixed(1).replace(".", ",")} s</span></div>
      </div>
      <div class="stat">
        <div class="head">Chyby <CircleX size={15} class="fault" /></div>
        <div class="val"><span class="readout n fault">{errors.length}</span><span class="sub">{errors.length ? "nepůjdou přehrát" : "žádná skladba"}</span></div>
      </div>
      <div class="stat">
        <div class="head">Varování <TriangleAlert size={15} class="warn" /></div>
        <div class="val"><span class="readout n warn">{warnings.length}</span><span class="sub">{warnings.length ? "pojedou, ale…" : "nic"}</span></div>
      </div>
      <div class="stat">
        <div class="head">Úklid <Brush size={15} /></div>
        <div class="val"><span class="readout n">{r.appleDouble}</span><span class="sub">souborů ._ od macOS</span></div>
      </div>
    </div>

    <div class="cols">
      <div class="col">
        <div class="title"><h3>Přehrávače</h3><span class="meta">Co na kterém pojede</span></div>
        <div class="card players">
          {#each r.players as p}
            {@const res = result(p)}
            <div class="player">
              <span class="tile"><Disc3 size={16} /></span>
              <span class="pname">{p.player}</span>
              <span class="chips">
                <span class="chip {p.filesystemOk ? 'ok' : 'fault'}" title={p.filesystemNote}>
                  <i></i>{p.filesystemOk ? fsName(r.filesystem) : p.filesystemNote || `${fsName(r.filesystem)} nepřečte`}
                </span>
                <span class="chip {p.formatNote ? 'fault' : 'ok'}"><i></i>{p.formatNote || "Všechny formáty"}</span>
              </span>
              <span class="res {res.cls}">
                {#if res.cls === "ok"}<CircleCheck size={15} />{:else if res.cls === "warn"}<TriangleAlert size={15} />{:else}<CircleX size={15} />{/if}
                {res.text}
              </span>
            </div>
          {/each}
        </div>
      </div>

      <div class="col">
        <div class="title"><h3>Je potřeba opravit</h3><span class="meta"><Lock size={11} /> Kontrola jen četla, nic se nezapsalo</span></div>
        {#if errors.length === 0 && warnings.length === 0 && r.appleDouble === 0}
          <div class="card fine"><CircleCheck size={16} /> Všechno je v pořádku, není co opravovat.</div>
        {/if}
        <div class="fixes">
          {#each errors as i, n (`${n}:${i.trackId}:${i.kind}`)}
            {@const t = app.trackById.get(i.trackId)}
            <div class="fix error">
              <span class="tile fault"><FileX2 size={16} /></span>
              <div class="what">
                <div class="ft">{t?.title ?? i.title}{#if t?.artist}<span class="artist"> · {t.artist}</span>{/if}</div>
                <div class="sub" title={i.message}>{i.message} · {i.playlists.length ? `v ${i.playlists.join(", ")}` : "v žádném playlistu"}</div>
              </div>
              <button class="btn icon ghost" onclick={() => api.reveal(abs(i.filePath))} title="Ukázat ve Finderu" aria-label="Ukázat ve Finderu"><FolderSearch size={15} /></button>
              <button class="btn" disabled title="Brzy"><Wrench size={14} /> Opravit…</button>
            </div>
          {/each}

          {#if warnings.length}
            <div class="fix">
              <button class="toggle" onclick={() => (showWarnings = !showWarnings)} aria-expanded={showWarnings}>
                <span class="tile warn"><TriangleAlert size={16} /></span>
                <div class="what">
                  <div class="ft">{warnings.length} {plural(warnings.length, "varování", "varování", "varování")}</div>
                  <div class="sub">jiná velikost souboru v databázi · hrát půjdou</div>
                </div>
                <ChevronDown size={16} class={showWarnings ? "chev open" : "chev"} />
              </button>
              {#if showWarnings}
                <ul>
                  {#each warnings as w, n (`${n}:${w.trackId}`)}
                    <li><span>{w.title}</span><span class="sub">{w.message}</span></li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}

          {#if r.appleDouble > 0}
            <div class="fix">
              <span class="tile"><Brush size={16} /></span>
              <div class="what">
                <div class="ft">{r.appleDouble} souborů ._</div>
                <div class="sub" title="Soubory ._ od macOS zbytečně zabírají místo a na CDJ vypadají jako rozbité skladby.">od macOS, zbytečné, na CDJ vypadají jako rozbité skladby</div>
              </div>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .health {
    flex: 1;
    overflow-y: auto;
    padding: 4px 28px 24px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .loading {
    margin: auto;
    display: flex;
    gap: 10px;
    align-items: center;
    color: var(--text-2);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }
  .stat {
    background: var(--surface-2);
    border-radius: var(--r-panel);
    padding: 16px 16px 14px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    color: var(--text-2);
    margin-bottom: 10px;
  }
  .head :global(svg) {
    color: var(--text-3);
  }
  .val {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .val .sub {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .n {
    font-size: 34px;
    line-height: 1;
    color: var(--text);
  }
  .n.fault,
  .health :global(svg.fault) {
    color: var(--danger);
  }
  .n.warn,
  .health :global(svg.warn) {
    color: var(--warning);
  }
  .sub {
    font-size: 12px;
    color: var(--text-3);
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 0.7fr);
    gap: 16px;
    align-items: start;
  }
  .title {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .meta {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-3);
  }
  .card {
    background: var(--surface-2);
    border-radius: var(--r-panel);
  }
  .player {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 57px;
    padding: 0 16px;
  }
  .player + .player {
    border-top: 1px solid var(--hairline);
  }
  .tile {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--surface-3);
    color: var(--text-2);
  }
  .tile.fault {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .tile.warn {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .pname {
    flex: 1;
    min-width: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .chips {
    display: flex;
    gap: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: 12px;
    font-size: 12px;
    white-space: nowrap;
    background: var(--surface-3);
    color: var(--text-2);
  }
  .chip i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: currentColor;
  }
  .chip.ok i {
    background: var(--success);
  }
  .chip.fault {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .res {
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 150px;
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
  }
  .res.ok {
    color: var(--success);
  }
  .res.warn {
    color: var(--warning);
  }
  .res.fault {
    color: var(--danger);
  }
  .fixes {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .fix {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    background: var(--surface-2);
    border-radius: var(--r-panel);
  }
  .fix.error {
    box-shadow: inset 0 0 0 1px rgb(240 88 92 / 0.22);
  }
  .what {
    flex: 1;
    min-width: 0;
  }
  .ft {
    font-size: 13.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist {
    color: var(--text-3);
    font-weight: 400;
  }
  .what .sub {
    margin-top: 2px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .toggle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 12px;
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
  }
  .toggle :global(.chev) {
    color: var(--text-3);
    transition: transform 140ms ease;
  }
  .toggle :global(.chev.open) {
    transform: rotate(180deg);
  }
  .fine {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px;
    color: var(--success);
  }
  ul {
    flex-basis: 100%;
    list-style: none;
    margin: 0;
    padding: 0 0 0 44px;
    max-height: 220px;
    overflow-y: auto;
  }
  li {
    display: flex;
    flex-direction: column;
    padding: 6px 0;
    border-top: 1px solid var(--hairline);
  }
</style>
