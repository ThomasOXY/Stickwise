<script lang="ts">
  import { ArrowLeftRight, Folder, ListMusic, TriangleAlert } from "@lucide/svelte";
  import { peerApi, type Deletion, type MirrorChanges } from "./api";
  import { plural, size } from "./format";
  import SheetCheck from "./SheetCheck.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  /** What mirroring with another Mac would change, for the DJ to confirm. */
  const r = $derived(app.mirrorRequest!);
  const p = $derived(r.preview);

  // Deletions are carried out unless unticked.
  let keepHere = $state<Set<string>>(new Set());
  let keepThere = $state<Set<string>>(new Set());

  function lines(c: MirrorChanges): string[] {
    const r = c.report;
    const out: string[] = [];
    if (r.tracksAdded) out.push(`+ ${r.tracksAdded} ${plural(r.tracksAdded, "nová skladba", "nové skladby", "nových skladeb")} · ${size(c.newBytes)}`);
    if (r.cuesUpdated) out.push(`novější cue u ${r.cuesUpdated} ${plural(r.cuesUpdated, "skladby", "skladeb", "skladeb")}`);
    const other = r.tracksUpdated - r.cuesUpdated;
    if (other > 0) out.push(`${other} ${plural(other, "skladba", "skladby", "skladeb")} s novějšími tagy, gridem nebo analýzou`);
    if (r.playlistsAdded) out.push(`+ ${r.playlistsAdded} ${plural(r.playlistsAdded, "playlist", "playlisty", "playlistů")}`);
    if (r.playlistsUpdated) out.push(`${r.playlistsUpdated} ${plural(r.playlistsUpdated, "upravený playlist", "upravené playlisty", "upravených playlistů")}`);
    return out;
  }

  const here = $derived(lines(p.here));
  const there = $derived(lines(p.there));
  const nothing = $derived(!here.length && !there.length && !p.deleteHere.length && !p.deleteThere.length);

  function day(ms: number): string {
    const d = new Date(ms);
    return `${d.getDate()}. ${d.getMonth() + 1}.`;
  }

  function toggle(set: Set<string>, uid: string): Set<string> {
    const next = new Set(set);
    if (next.has(uid)) next.delete(uid);
    else next.add(uid);
    return next;
  }

  async function answer(go: boolean) {
    const req = r.request;
    const choice = go
      ? {
          deleteHere: p.deleteHere.filter((d) => !keepHere.has(d.uid)).map((d) => d.uid),
          deleteThere: p.deleteThere.filter((d) => !keepThere.has(d.uid)).map((d) => d.uid),
        }
      : null;
    app.mirrorRequest = null;
    await peerApi.mirrorAnswer(req, choice);
  }
</script>

{#snippet deletions(list: Deletion[], keep: Set<string>, flip: (uid: string) => void, where: string)}
  <div class="pick">
    {#each list as d (d.uid)}
      <button class="row" onclick={() => flip(d.uid)}>
        <SheetCheck state={keep.has(d.uid) ? "off" : "on"} />
        {#if d.isFolder}<Folder size={14} />{:else}<ListMusic size={14} />{/if}
        <span class="pname">{d.name}</span>
        <span class="pcount">{where} {day(d.deleted)}</span>
      </button>
    {/each}
  </div>
{/snippet}

<SheetFrame id="mirror-title" icon={ArrowLeftRight} title="Zrcadlení s {r.peer}" sub="Obě knihovny budou stejné" width={680} onclose={() => answer(false)}>
  <div class="body">
    {#if nothing}
      <p class="same">Knihovny jsou stejné, není co synchronizovat.</p>
    {:else}
      <div class="cols">
        {#each [{ title: "Tento Mac", list: here }, { title: r.peer, list: there }] as side (side.title)}
          <div class="side">
            <div class="eyebrow">{side.title}</div>
            {#if side.list.length}
              {#each side.list as l (l)}<div class="line">{l}</div>{/each}
            {:else}
              <div class="line quiet">beze změn</div>
            {/if}
          </div>
        {/each}
      </div>

      {#if p.deleteHere.length}
        <section>
          <div class="eyebrow">Smazané na {r.peer} — smazat i tady?</div>
          {@render deletions(p.deleteHere, keepHere, (u) => (keepHere = toggle(keepHere, u)), "smazáno")}
        </section>
      {/if}
      {#if p.deleteThere.length}
        <section>
          <div class="eyebrow">Smazané tady — smazat i na {r.peer}?</div>
          {@render deletions(p.deleteThere, keepThere, (u) => (keepThere = toggle(keepThere, u)), "smazáno")}
        </section>
      {/if}
      {#if p.deleteHere.length || p.deleteThere.length}
        <p class="hint">Co odškrtneš, se nesmaže a vrátí se na Mac, kde bylo smazané. Skladby ani soubory se nemažou nikdy.</p>
      {/if}
    {/if}

    {#if p.here.leftOut.length || p.there.leftOut.length}
      <p class="warn">
        <TriangleAlert size={15} />Bez playlistů se skladbami na odpojeném disku:
        {[...p.here.leftOut, ...p.there.leftOut].join(", ")}
      </p>
    {/if}
    {#if !p.roomHere}
      <p class="err"><TriangleAlert size={15} />Na disku pro hudbu není dost místa ({size(p.here.newBytes)}). Uvolni místo nebo změň složku v Sync mezi Macy.</p>
    {/if}
    <p class="hint">Kde je úprava novější, vyhrává. Soubory, které už na Macu jsou, se nepřepíšou; přepsané cue jdou vrátit.</p>
  </div>

  {#snippet foot()}
    <button class="btn ghost lg" onclick={() => answer(false)}>{nothing ? "Zavřít" : "Zrušit"}</button>
    <button class="btn primary lg" onclick={() => answer(true)} disabled={!p.roomHere || nothing}>
      <ArrowLeftRight size={16} />Synchronizovat
    </button>
  {/snippet}
</SheetFrame>

<style>
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px 26px 24px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .side {
    padding: 14px 16px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .side .eyebrow {
    margin: 0 0 4px;
  }
  .line {
    font-size: 13.5px;
    color: var(--text);
  }
  .quiet {
    color: var(--text-3);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pick {
    max-height: 200px;
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
    height: 34px;
    flex: none;
    padding: 0 12px 0 10px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-3);
  }
  .pname {
    flex: 1;
    min-width: 0;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pcount {
    font-size: 12px;
    color: var(--text-3);
  }
  .same {
    margin: 8px 0;
    font-size: 15px;
    color: var(--text-2);
  }
  .hint,
  .warn,
  .err {
    margin: 0;
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-3);
  }
  .warn {
    color: var(--warning);
  }
  .err {
    color: var(--danger);
  }
  .warn :global(svg),
  .err :global(svg) {
    flex: none;
    margin-top: 1px;
  }
</style>
