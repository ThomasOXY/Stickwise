<script lang="ts">
  import { Search } from "@lucide/svelte";
  import { keyColor, plural, rgb, time } from "./format";
  import { app } from "./state.svelte";

  const total = $derived(app.currentTracks.reduce((s, t) => s + t.duration, 0));
  const bad = $derived(app.currentTracks.filter((t) => app.errorTracks.has(t.id)).length);

  function statusClass(id: number): string {
    if (!app.verify) return "unknown";
    const s = app.status.get(id);
    return s === "error" ? "fault" : s === "warning" ? "warn" : "ok";
  }

  function onKey(e: KeyboardEvent) {
    const list = app.currentTracks;
    if (list.length === 0) return;
    const i = list.findIndex((t) => t.id === app.selectedTrack);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = list[Math.min(list.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)))];
      app.selectTrack(next.id);
      document.getElementById(`row-${next.id}`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && app.selectedTrack != null) {
      app.detailOpen = true;
    }
  }
</script>

<section class="table">
  <div class="title">
    <h2>{app.currentTitle}</h2>
    <span><span class="readout big">{app.currentTracks.length}</span> {plural(app.currentTracks.length, "skladba", "skladby", "skladeb")}</span>
    <span><span class="readout big">{time(total)}</span> celkem</span>
    {#if bad > 0}
      <span><span class="readout big fault">{bad}</span> {plural(bad, "nepůjde", "nepůjdou", "nepůjde")}</span>
    {/if}
    <label class="search">
      <Search size={14} />
      <input type="search" placeholder="Hledat v playlistu" bind:value={app.trackFilter} />
    </label>
  </div>

  <div class="cols">
    <span class="c-no">#</span>
    <span class="c-st"></span>
    <span class="c-title">Název</span>
    <span class="c-artist">Interpret</span>
    <span class="c-bpm r">BPM</span>
    <span class="c-key">Tónina</span>
    <span class="c-time r">Čas</span>
    <span class="c-fmt">Formát</span>
    <span class="c-cue">Hot cue</span>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="rows" tabindex="0" role="listbox" aria-label="Skladby" onkeydown={onKey}>
    {#each app.currentTracks as t, i (t.id)}
      {@const st = statusClass(t.id)}
      {@const fmt = app.formatOverride.get(t.id) ?? t.format}
      {@const sev = app.status.get(t.id)}
      <div
        id="row-{t.id}"
        class="row"
        class:selected={t.id === app.selectedTrack}
        role="option"
        aria-selected={t.id === app.selectedTrack}
        tabindex="-1"
        onclick={() => app.selectTrack(t.id)}
        ondblclick={() => {
          app.selectTrack(t.id);
          app.detailOpen = true;
        }}
        onkeydown={() => {}}
      >
        <span class="c-no readout" class:cur={t.id === app.selectedTrack}>{i + 1}</span>
        <span class="c-st"><span class="dot {st}"></span></span>
        <span class="c-title">{t.title}</span>
        <span class="c-artist">{t.artist}</span>
        <span class="c-bpm r readout num">{t.bpm.toFixed(2)}</span>
        <span class="c-key"><i style:background={keyColor(t.key)}></i><span class="readout num">{t.key}</span></span>
        <span class="c-time r readout num">{time(t.duration)}</span>
        <span class="c-fmt" class:fault={sev === "error"} class:warn={sev === "warning"}>{fmt}</span>
        <span class="c-cue">
          {#each Array(8) as _, s}
            {@const c = app.cues[t.id]?.[s]}
            <i class:off={!c} style:background={c ? rgb(c, s + 1) : undefined}></i>
          {/each}
        </span>
      </div>
    {/each}
    {#if app.currentTracks.length === 0}
      <p class="empty">
        {app.selection?.kind === "player" ? "Na tomhle přehrávači půjde všechno." : "Playlist je prázdný."}
      </p>
    {/if}
  </div>
</section>

<style>
  .table {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 14px;
    padding: 14px 16px 12px;
    color: var(--text-2);
    font-size: 12px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }
  .big {
    font-size: 16px;
    color: var(--text);
  }
  .big.fault {
    color: var(--fault);
  }
  .search {
    margin-left: auto;
    align-self: center;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 200px;
    padding: 0 10px;
    height: 28px;
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--text-2);
  }
  .search:focus-within {
    border-color: var(--cue);
  }
  .search input {
    border: 0;
    height: 26px;
    padding: 0;
    flex: 1;
    min-width: 0;
    background: transparent;
  }
  .cols,
  .row {
    display: grid;
    grid-template-columns: 24px 16px minmax(160px, 1fr) minmax(120px, 0.62fr) 52px 56px 40px 84px 92px;
    gap: 8px;
    align-items: center;
    padding: 0 16px;
  }
  .cols {
    height: 30px;
    font-size: 11px;
    color: var(--text-2);
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .rows {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    outline: none;
  }
  .row {
    height: 28px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
  }
  .row:hover {
    background: rgb(255 255 255 / 0.025);
  }
  .row.selected {
    background: var(--panel-2);
  }
  .r {
    text-align: right;
  }
  .c-no {
    text-align: right;
    color: var(--text-2);
  }
  .c-no.cur {
    color: var(--cue);
  }
  .c-st {
    display: flex;
    justify-content: center;
  }
  .c-title {
    font-weight: 500;
  }
  .c-title,
  .c-artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-artist {
    color: var(--text-2);
  }
  .num {
    font-size: 15px;
  }
  .c-key {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 12px;
  }
  .c-key i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: none;
  }
  .c-fmt {
    padding-left: 16px;
    font-size: 12px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .c-fmt.fault {
    color: var(--fault);
  }
  .c-fmt.warn {
    color: var(--warn);
  }
  .c-cue {
    display: flex;
    gap: 3px;
  }
  .c-cue i {
    width: 7px;
    height: 7px;
    border-radius: 1px;
  }
  .c-cue i.off {
    background: var(--line);
  }
  .empty {
    padding: 24px 16px;
    color: var(--text-2);
  }
</style>
